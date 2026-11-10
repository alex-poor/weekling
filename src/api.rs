//! JSON admin API for the management UI, under `/api`.
//!
//! Auth is cookie-based sessions (distinct from CalDAV's Basic app passwords):
//! `POST /api/login` with the account password sets an HttpOnly `wk_session`
//! cookie and returns a CSRF token the SPA echoes in `X-CSRF-Token` on writes.

use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router, middleware};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{hash_secret, verify_secret};
use crate::http::AppState;
use crate::store::{Access, Calendar, CredentialInfo, Principal};
use crate::util;

/// Session length: 14 days.
const SESSION_HOURS: i64 = 24 * 14;
const COOKIE: &str = "wk_session";

/// The principal behind a valid session, attached by [`require_session`].
#[derive(Clone)]
struct SessionUser(Principal);

pub fn router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/api/me", get(me))
        .route("/api/logout", post(logout))
        .route("/api/calendars", get(list_calendars).post(create_calendar))
        .route(
            "/api/calendars/{id}",
            patch(update_calendar).delete(delete_calendar),
        )
        .route("/api/tokens", get(list_tokens).post(create_token))
        .route("/api/tokens/{id}", delete(delete_token))
        .route("/api/principals", get(list_principals))
        .route("/api/shared", get(shared_with_me))
        .route(
            "/api/calendars/{id}/shares",
            get(list_shares).post(add_share),
        )
        .route("/api/calendars/{id}/shares/{grantee}", delete(remove_share))
        .route("/api/inbox", get(list_inbox))
        .route("/api/inbox/{id}/reply", post(reply_inbox))
        .layer(middleware::from_fn_with_state(state, require_session));

    Router::new()
        .route("/api/login", post(login))
        .route("/api/auth-info", get(auth_info))
        .merge(protected)
}

/// Public: what sign-in methods the UI should offer.
async fn auth_info(State(state): State<AppState>) -> Response {
    Json(json!({ "oidc": state.oidc.is_some() })).into_response()
}

/// Create a session and return the two Set-Cookie headers (session + CSRF).
/// Shared by password login and OIDC callback. `AppendHeaders` emits both
/// Set-Cookie headers — a plain array would collapse them to the last one.
pub(crate) fn issue_session(
    store: &crate::store::Store,
    principal_id: &str,
) -> anyhow::Result<AppendHeaders<[(header::HeaderName, String); 2]>> {
    let sid = util::new_secret();
    let csrf = util::new_secret();
    store.create_session(&sid, principal_id, &csrf, SESSION_HOURS)?;
    let max_age = SESSION_HOURS * 3600;
    Ok(AppendHeaders([
        (
            header::SET_COOKIE,
            format!("{COOKIE}={sid}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}"),
        ),
        (
            header::SET_COOKIE,
            format!("wk_csrf={csrf}; SameSite=Lax; Path=/; Max-Age={max_age}"),
        ),
    ]))
}

// ---- auth ---------------------------------------------------------------

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(State(state): State<AppState>, Json(req): Json<LoginReq>) -> Response {
    let Ok(Some(principal)) = state.store.get_principal(&req.username) else {
        return unauthorized();
    };
    let Ok(Some(hash)) = state.store.password_hash(&principal.id) else {
        return unauthorized();
    };
    let pw = req.password;
    let ok = tokio::task::spawn_blocking(move || verify_secret(&pw, &hash))
        .await
        .unwrap_or(false);
    if !ok {
        return unauthorized();
    }

    let cookies = match issue_session(&state.store, &principal.id) {
        Ok(c) => c,
        Err(_) => return server_error(),
    };
    (
        StatusCode::OK,
        cookies,
        Json(json!({ "user": user_json(&principal) })),
    )
        .into_response()
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(sid) = cookie_value(&headers, COOKIE) {
        let _ = state.store.delete_session(&sid);
    }
    let clear = format!("{COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    let clear_csrf = "wk_csrf=; SameSite=Lax; Path=/; Max-Age=0".to_string();
    (
        StatusCode::NO_CONTENT,
        AppendHeaders([
            (header::SET_COOKIE, clear),
            (header::SET_COOKIE, clear_csrf),
        ]),
    )
        .into_response()
}

async fn me(Extension(SessionUser(p)): Extension<SessionUser>) -> Response {
    Json(json!({ "user": user_json(&p) })).into_response()
}

/// Middleware: require a live session; enforce CSRF on mutating methods.
async fn require_session(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(sid) = cookie_value(req.headers(), COOKIE) else {
        return unauthorized();
    };
    let Ok(Some((pid, csrf))) = state.store.get_session(&sid) else {
        return unauthorized();
    };
    if is_mutating(req.method()) {
        let sent = req
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok());
        if sent != Some(csrf.as_str()) {
            return (StatusCode::FORBIDDEN, "invalid CSRF token\n").into_response();
        }
    }
    let Ok(Some(principal)) = state.store.get_principal(&pid) else {
        return unauthorized();
    };
    req.extensions_mut().insert(SessionUser(principal));
    next.run(req).await
}

// ---- calendars ----------------------------------------------------------

async fn list_calendars(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
) -> Response {
    match state.store.list_calendars(&p.id) {
        Ok(cals) => Json(cals.iter().map(cal_json).collect::<Vec<_>>()).into_response(),
        Err(_) => server_error(),
    }
}

#[derive(Deserialize)]
struct NewCal {
    display_name: String,
    #[serde(default)]
    components: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

async fn create_calendar(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Json(req): Json<NewCal>,
) -> Response {
    let name = req.display_name.trim();
    if name.is_empty() {
        return bad_request("display_name is required");
    }
    let components = req.components.unwrap_or_else(|| "VEVENT".into());
    if !valid_components(&components) {
        return bad_request("components must be VEVENT, VTODO, or VEVENT,VTODO");
    }
    let id = util::new_id();
    if state
        .store
        .create_calendar(
            &id,
            &p.id,
            name,
            &components,
            req.color.as_deref(),
            req.description.as_deref(),
        )
        .is_err()
    {
        return server_error();
    }
    match state.store.get_calendar(&p.id, &id) {
        Ok(Some(c)) => (StatusCode::CREATED, Json(cal_json(&c))).into_response(),
        _ => server_error(),
    }
}

#[derive(Deserialize)]
struct PatchCal {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

async fn update_calendar(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(req): Json<PatchCal>,
) -> Response {
    match state.store.update_calendar(
        &p.id,
        &id,
        req.display_name.as_deref(),
        req.color.as_deref(),
        req.description.as_deref(),
    ) {
        Ok(true) => match state.store.get_calendar(&p.id, &id) {
            Ok(Some(c)) => Json(cal_json(&c)).into_response(),
            _ => server_error(),
        },
        Ok(false) => not_found(),
        Err(_) => server_error(),
    }
}

async fn delete_calendar(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path(id): Path<String>,
) -> Response {
    match state.store.delete_calendar(&p.id, &id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(),
        Err(_) => server_error(),
    }
}

// ---- app passwords ------------------------------------------------------

async fn list_tokens(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
) -> Response {
    match state.store.list_app_passwords(&p.id) {
        Ok(list) => Json(list.iter().map(token_json).collect::<Vec<_>>()).into_response(),
        Err(_) => server_error(),
    }
}

#[derive(Deserialize)]
struct NewToken {
    #[serde(default)]
    label: Option<String>,
}

async fn create_token(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Json(req): Json<NewToken>,
) -> Response {
    let label = req.label.unwrap_or_else(|| "app password".into());
    let token = util::new_token();
    let tclone = token.clone();
    let hash = match tokio::task::spawn_blocking(move || hash_secret(&tclone)).await {
        Ok(Ok(h)) => h,
        _ => return server_error(),
    };
    let id = util::new_id();
    if state
        .store
        .add_app_password(&id, &p.id, &label, &hash)
        .is_err()
    {
        return server_error();
    }
    // The token is returned exactly once.
    (
        StatusCode::CREATED,
        Json(json!({ "id": id, "label": label, "token": token })),
    )
        .into_response()
}

async fn delete_token(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path(id): Path<String>,
) -> Response {
    match state.store.delete_app_password(&p.id, &id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(),
        Err(_) => server_error(),
    }
}

// ---- sharing ------------------------------------------------------------

async fn list_principals(
    State(state): State<AppState>,
    Extension(SessionUser(_p)): Extension<SessionUser>,
) -> Response {
    match state.store.list_users() {
        Ok(users) => Json(
            users
                .iter()
                .map(|(id, dn)| json!({ "id": id, "display_name": dn }))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => server_error(),
    }
}

async fn shared_with_me(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
) -> Response {
    match state.store.shared_with(&p.id) {
        Ok(list) => Json(
            list.iter()
                .map(|(c, acc)| {
                    json!({
                        "id": c.id,
                        "owner": c.owner_id,
                        "display_name": c.display_name,
                        "color": c.color,
                        "components": c.components.split(',').collect::<Vec<_>>(),
                        "access": access_str(*acc),
                        "url": format!("/dav/calendars/{}/{}/", c.owner_id, c.id),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => server_error(),
    }
}

async fn list_shares(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path(id): Path<String>,
) -> Response {
    match state.store.list_shares(&p.id, &id) {
        Ok(shares) => {
            let out: Vec<_> = shares
                .iter()
                .map(|(g, a)| {
                    let name = state
                        .store
                        .get_principal(g)
                        .ok()
                        .flatten()
                        .map(|pr| pr.display_name)
                        .unwrap_or_else(|| g.clone());
                    json!({ "grantee": g, "grantee_name": name, "access": a })
                })
                .collect();
            Json(out).into_response()
        }
        Err(_) => server_error(),
    }
}

#[derive(Deserialize)]
struct NewShare {
    grantee: String,
    #[serde(default)]
    access: Option<String>,
}

async fn add_share(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(req): Json<NewShare>,
) -> Response {
    let access = req.access.unwrap_or_else(|| "read".into());
    if !matches!(access.as_str(), "read" | "read-write" | "freebusy") {
        return bad_request("access must be read, read-write, or freebusy");
    }
    match state
        .store
        .share_calendar(&p.id, &id, &req.grantee, &access)
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => bad_request("could not share — check the calendar and that the user exists"),
        Err(_) => server_error(),
    }
}

async fn remove_share(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path((id, grantee)): Path<(String, String)>,
) -> Response {
    match state.store.unshare(&p.id, &id, &grantee) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(),
        Err(_) => server_error(),
    }
}

// ---- scheduling inbox ---------------------------------------------------

async fn list_inbox(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
) -> Response {
    let inbox = crate::store::inbox_id(&p.id);
    match state.store.list_objects(&inbox, None) {
        Ok(items) => {
            let out: Vec<_> = items
                .iter()
                .map(|o| {
                    let f = crate::ical::extract(&o.ical_raw);
                    let mine = f.attendees.iter().find(|a| {
                        crate::schedule::resolve(&state, &a.value).as_deref() == Some(&p.id)
                    });
                    json!({
                        "id": o.uid,
                        "summary": f.summary,
                        "organizer": f.organizer.as_deref().map(strip_mailto),
                        "method": f.method,
                        "dtstart": o.dtstart,
                        "partstat": mine.and_then(|a| a.partstat.clone()),
                    })
                })
                .collect();
            Json(out).into_response()
        }
        Err(_) => server_error(),
    }
}

#[derive(Deserialize)]
struct Reply {
    partstat: String,
}

async fn reply_inbox(
    State(state): State<AppState>,
    Extension(SessionUser(p)): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(req): Json<Reply>,
) -> Response {
    let ps = req.partstat.to_ascii_uppercase();
    if !matches!(ps.as_str(), "ACCEPTED" | "DECLINED" | "TENTATIVE") {
        return bad_request("partstat must be ACCEPTED, DECLINED, or TENTATIVE");
    }
    if crate::schedule::respond(&state, &p.id, &id, &ps) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        not_found()
    }
}

fn strip_mailto(addr: &str) -> String {
    addr.strip_prefix("mailto:")
        .or_else(|| addr.strip_prefix("MAILTO:"))
        .unwrap_or(addr)
        .to_string()
}

fn access_str(a: Access) -> &'static str {
    match a {
        Access::Owner => "owner",
        Access::ReadWrite => "read-write",
        Access::Read => "read",
        Access::FreeBusy => "freebusy",
    }
}

// ---- helpers ------------------------------------------------------------

fn user_json(p: &Principal) -> Value {
    json!({ "id": p.id, "display_name": p.display_name, "is_admin": p.is_admin })
}

fn cal_json(c: &Calendar) -> Value {
    json!({
        "id": c.id,
        "display_name": c.display_name,
        "description": c.description,
        "color": c.color,
        "components": c.components.split(',').collect::<Vec<_>>(),
        "sync_token": c.sync_token,
        "url": format!("/dav/calendars/{}/{}/", c.owner_id, c.id),
    })
}

fn token_json(c: &CredentialInfo) -> Value {
    json!({
        "id": c.id,
        "label": c.label,
        "created_at": c.created_at,
        "last_used_at": c.last_used_at,
    })
}

fn valid_components(c: &str) -> bool {
    !c.is_empty() && c.split(',').all(|p| matches!(p, "VEVENT" | "VTODO"))
}

fn is_mutating(m: &Method) -> bool {
    matches!(
        *m,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Extract a cookie value from the `Cookie` header.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some(v) = pair.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "authentication required\n").into_response()
}
fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found\n").into_response()
}
fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, format!("{msg}\n")).into_response()
}
fn server_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response()
}
