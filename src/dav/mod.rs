//! CalDAV/WebDAV HTTP surface mounted under `/dav`.
//!
//! One fallback handler receives every method (including PROPFIND, REPORT,
//! MKCALENDAR, PROPPATCH) and dispatches by method and parsed resource path.

mod propfind;
mod report;
pub mod xml;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Extension, middleware};

use crate::auth::{AuthedPrincipal, require_auth};
use crate::http::AppState;
use crate::store::{Access, NewObject};
use crate::{ical, util};

/// The resource addressed by a `/dav/...` path.
#[derive(Debug, Clone)]
pub enum Resource {
    Root,
    Principal {
        user: String,
    },
    Home {
        user: String,
    },
    Calendar {
        user: String,
        cal: String,
    },
    Object {
        user: String,
        cal: String,
        name: String,
    },
    PushSub {
        id: String,
    },
    Unknown,
}

pub fn parse_path(path: &str) -> Resource {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        ["dav"] => Resource::Root,
        ["dav", "push", id] => Resource::PushSub {
            id: (*id).to_string(),
        },
        ["dav", "principals", user] => Resource::Principal {
            user: (*user).to_string(),
        },
        ["dav", "calendars", user] => Resource::Home {
            user: (*user).to_string(),
        },
        ["dav", "calendars", user, cal] => Resource::Calendar {
            user: (*user).to_string(),
            cal: (*cal).to_string(),
        },
        ["dav", "calendars", user, cal, obj] => Resource::Object {
            user: (*user).to_string(),
            cal: (*cal).to_string(),
            name: obj.strip_suffix(".ics").unwrap_or(obj).to_string(),
        },
        _ => Resource::Unknown,
    }
}

// ---- href helpers -------------------------------------------------------

pub fn principal_href(user: &str) -> String {
    format!("/dav/principals/{user}/")
}
pub fn home_href(user: &str) -> String {
    format!("/dav/calendars/{user}/")
}
pub fn calendar_href(user: &str, cal: &str) -> String {
    format!("/dav/calendars/{user}/{cal}/")
}
pub fn object_href(user: &str, cal: &str, name: &str) -> String {
    format!("/dav/calendars/{user}/{cal}/{name}.ics")
}

/// Which user does this resource belong to?
fn resource_user(r: &Resource) -> Option<&str> {
    match r {
        Resource::Principal { user }
        | Resource::Home { user }
        | Resource::Calendar { user, .. }
        | Resource::Object { user, .. } => Some(user),
        _ => None,
    }
}

/// Returns a `Router<AppState>` owning the `/dav` subtree. The caller `merge`s
/// it into the top-level router (which supplies the state). Explicit routes —
/// rather than `nest` — so that the bare `/dav/` root is also matched. `any`
/// dispatches every method, including PROPFIND/REPORT/MKCALENDAR/PROPPATCH.
pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/dav", any(dav_entry))
        .route("/dav/", any(dav_entry))
        .route("/dav/{*rest}", any(dav_entry))
        .layer(middleware::from_fn_with_state(state, require_auth))
}

async fn dav_entry(
    State(state): State<AppState>,
    Extension(AuthedPrincipal(principal)): Extension<AuthedPrincipal>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let resource = parse_path(uri.path());

    // Push subscription resources (/dav/push/<id>): the owner may remove them.
    if let Resource::PushSub { id } = &resource {
        return match method.as_str() {
            "OPTIONS" => options_response(),
            "DELETE" => match state.store.subscription_owner(id) {
                Ok(Some(owner)) if owner == principal.id => {
                    match state.store.delete_subscription(id, &principal.id) {
                        Ok(true) => StatusCode::NO_CONTENT.into_response(),
                        Ok(false) => not_found(),
                        Err(e) => server_error(e),
                    }
                }
                Ok(Some(_)) => forbidden(),
                Ok(None) => not_found(),
                Err(e) => server_error(e),
            },
            _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n").into_response(),
        };
    }

    // MKCALENDAR creates a new (not-yet-existing) calendar: only in your own home.
    if method.as_str() == "MKCALENDAR" {
        return match &resource {
            Resource::Calendar { user, .. } if *user == principal.id => {
                mkcalendar(&state, &resource, &body)
            }
            _ => forbidden(),
        };
    }

    // Effective access to the addressed resource (sharing-aware).
    let access = match &resource {
        Resource::Root => Some(Access::Owner),
        Resource::Principal { user } | Resource::Home { user } => {
            (*user == principal.id).then_some(Access::Owner)
        }
        Resource::Calendar { user, cal } | Resource::Object { user, cal, .. } => {
            match state.store.access_for(&principal.id, user, cal) {
                Ok(a) => a,
                Err(e) => return server_error(e),
            }
        }
        Resource::PushSub { .. } | Resource::Unknown => None, // PushSub handled above
    };
    let Some(access) = access else {
        // Don't distinguish "forbidden" from "absent" for another user's tree.
        return not_found();
    };

    match method.as_str() {
        "OPTIONS" => options_response(),
        "PROPFIND" => propfind::handle(&state, &principal, &resource, &headers, &body, access),
        "REPORT" => report::handle(&state, &principal, &resource, &body, access),
        "GET" | "HEAD" if access.can_read() => get(&state, &resource, method == Method::HEAD),
        "PUT" if access.can_write() => put(&state, &principal, &resource, &headers, &body),
        "DELETE" if access.can_write() => delete(&state, &principal, &resource, &headers),
        "PROPPATCH" if access.can_write() => proppatch(&state, &resource, &body),
        // WebDAV-Push registration: subscribers must be able to read the collection.
        "POST" if access.can_read() => push_register(&state, &principal, &resource, &body),
        "GET" | "HEAD" | "PUT" | "DELETE" | "PROPPATCH" | "POST" => forbidden(),
        _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n").into_response(),
    }
}

fn options_response() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (
                header::ALLOW,
                "OPTIONS, GET, HEAD, POST, PUT, DELETE, PROPFIND, PROPPATCH, REPORT, MKCALENDAR",
            ),
            (
                header::HeaderName::from_static("dav"),
                "1, 3, calendar-access, webdav-push",
            ),
        ],
    )
        .into_response()
}

fn get(state: &AppState, resource: &Resource, head: bool) -> Response {
    let Resource::Object { cal, name, .. } = resource else {
        return (StatusCode::METHOD_NOT_ALLOWED, "not a document\n").into_response();
    };
    match state.store.get_object(cal, name) {
        Ok(Some(obj)) => {
            let headers = [
                (
                    header::CONTENT_TYPE,
                    "text/calendar; charset=utf-8".to_string(),
                ),
                (header::ETAG, obj.etag.clone()),
            ];
            let body = if head { String::new() } else { obj.ical_raw };
            (StatusCode::OK, headers, body).into_response()
        }
        Ok(None) => not_found(),
        Err(e) => server_error(e),
    }
}

fn put(
    state: &AppState,
    principal: &crate::store::Principal,
    resource: &Resource,
    headers: &HeaderMap,
    body: &Bytes,
) -> Response {
    let Resource::Object { cal, name, .. } = resource else {
        return (StatusCode::METHOD_NOT_ALLOWED, "cannot PUT here\n").into_response();
    };
    // Calendar must exist and be owned by the (already-authorized) user.
    let Some(user) = resource_user(resource) else {
        return not_found();
    };
    let cal_row = match state.store.get_calendar(user, cal) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found(),
        Err(e) => return server_error(e),
    };

    let Ok(text) = std::str::from_utf8(body) else {
        return (StatusCode::BAD_REQUEST, "body is not UTF-8\n").into_response();
    };
    if let Err(msg) = ical::validate(text) {
        return (
            StatusCode::BAD_REQUEST,
            format!("invalid iCalendar: {msg}\n"),
        )
            .into_response();
    }
    let fields = ical::extract(text);

    let existing = match state.store.get_object(cal, name) {
        Ok(v) => v,
        Err(e) => return server_error(e),
    };

    // Preconditions.
    if header_has(headers, "if-none-match", "*") && existing.is_some() {
        return (StatusCode::PRECONDITION_FAILED, "already exists\n").into_response();
    }
    if let Some(want) = header_value(headers, "if-match") {
        match &existing {
            Some(obj) if etag_matches(&want, &obj.etag) => {}
            _ => return (StatusCode::PRECONDITION_FAILED, "etag mismatch\n").into_response(),
        }
    }

    let etag = util::etag_for(body);
    let new = NewObject {
        collection_id: cal,
        uid: name,
        etag: &etag,
        ical_raw: text,
        dtstart: fields.dtstart.as_deref(),
        dtend: fields.dtend.as_deref(),
        summary: fields.summary.as_deref(),
        status: fields.status.as_deref(),
        component: fields.component.as_deref(),
        has_rrule: fields.has_rrule,
    };
    let seq = match state.store.put_object(&new, &util::new_id()) {
        Ok(seq) => seq,
        Err(e) => return server_error(e),
    };
    crate::push::notify(state, cal, seq);
    // Implicit scheduling for events written to a normal calendar (not the inbox).
    if cal_row.role.is_none() {
        crate::schedule::on_put(state, &principal.id, text);
    }

    let code = if existing.is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CREATED
    };
    (code, [(header::ETAG, etag)]).into_response()
}

fn delete(
    state: &AppState,
    principal: &crate::store::Principal,
    resource: &Resource,
    headers: &HeaderMap,
) -> Response {
    let Resource::Object { cal, name, user } = resource else {
        return (StatusCode::METHOD_NOT_ALLOWED, "cannot DELETE here\n").into_response();
    };
    // Capture the object (for an If-Match check and for a scheduling CANCEL).
    let existing = match state.store.get_object(cal, name) {
        Ok(v) => v,
        Err(e) => return server_error(e),
    };
    if let Some(want) = header_value(headers, "if-match") {
        match &existing {
            Some(obj) if etag_matches(&want, &obj.etag) => {}
            Some(_) => return (StatusCode::PRECONDITION_FAILED, "etag mismatch\n").into_response(),
            None => return not_found(),
        }
    }
    let is_inbox = matches!(state.store.get_calendar(user, cal), Ok(Some(c)) if c.role.is_some());

    match state.store.delete_object(cal, name) {
        Ok(true) => {
            if let Ok(seq) = state.store.collection_sync_token(cal) {
                crate::push::notify(state, cal, seq);
            }
            if !is_inbox && let Some(obj) = existing {
                crate::schedule::on_delete(state, &principal.id, &obj.ical_raw);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => not_found(),
        Err(e) => server_error(e),
    }
}

/// WebDAV-Push `push-register`: store a Web Push subscription for a calendar
/// and return its registration URL (Location) and expiry.
fn push_register(
    state: &AppState,
    principal: &crate::store::Principal,
    resource: &Resource,
    body: &Bytes,
) -> Response {
    let Resource::Calendar { cal, .. } = resource else {
        return (StatusCode::FORBIDDEN, "subscribe on a calendar\n").into_response();
    };
    let reg = xml::parse_push_register(body);
    if reg.push_resource.is_empty() || reg.p256dh.is_empty() || reg.auth_secret.is_empty() {
        return push_precondition("invalid-subscription");
    }

    let id = util::new_id();
    let effective = match state.store.upsert_subscription(
        &id,
        cal,
        &principal.id,
        &reg.push_resource,
        &reg.p256dh,
        &reg.auth_secret,
        crate::push::EXPIRY_MODIFIER,
    ) {
        Ok(e) => e,
        Err(e) => return server_error(e),
    };

    let location = format!("/dav/push/{effective}");
    let expires = httpdate::fmt_http_date(
        std::time::SystemTime::now() + std::time::Duration::from_secs(30 * 24 * 60 * 60),
    );
    (
        StatusCode::NO_CONTENT,
        [(header::LOCATION, location), (header::EXPIRES, expires)],
    )
        .into_response()
}

/// A 403 with a WebDAV-Push precondition error element.
fn push_precondition(code: &str) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:error xmlns:d=\"DAV:\" xmlns:P=\"https://bitfire.at/webdav-push\"><P:{code}/></d:error>"
    );
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
        .into_response()
}

fn mkcalendar(state: &AppState, resource: &Resource, body: &Bytes) -> Response {
    let Resource::Calendar { user, cal } = resource else {
        return (StatusCode::FORBIDDEN, "cannot create a calendar here\n").into_response();
    };
    if matches!(state.store.get_calendar(user, cal), Ok(Some(_))) {
        return (StatusCode::METHOD_NOT_ALLOWED, "already exists\n").into_response();
    }
    let props = xml::parse_mkcalendar(body);
    let components = props.components.unwrap_or_else(|| "VEVENT".to_string());
    let display = props.display_name.unwrap_or_else(|| cal.clone());
    if let Err(e) = state.store.create_calendar(
        cal,
        user,
        &display,
        &components,
        props.color.as_deref(),
        props.description.as_deref(),
    ) {
        return server_error(e);
    }
    StatusCode::CREATED.into_response()
}

fn proppatch(state: &AppState, resource: &Resource, body: &Bytes) -> Response {
    let Resource::Calendar { user, cal } = resource else {
        return (StatusCode::FORBIDDEN, "cannot set properties here\n").into_response();
    };
    let props = xml::parse_mkcalendar(body); // same set/prop shape
    let _ = state.store.update_calendar(
        user,
        cal,
        props.display_name.as_deref(),
        props.color.as_deref(),
        props.description.as_deref(),
    );
    // Report the touched props as successfully set.
    let mut set = String::new();
    for (name, prefix) in [
        (props.display_name.as_ref().map(|_| "displayname"), "d"),
        (props.color.as_ref().map(|_| "calendar-color"), "ical"),
        (
            props.description.as_ref().map(|_| "calendar-description"),
            "cal",
        ),
    ] {
        if let Some(n) = name {
            set.push_str(&format!("<{prefix}:{n}/>"));
        }
    }
    let href = calendar_href(user, cal);
    let resp = xml::response(
        &href,
        &format!(
            "<d:propstat><d:prop>{set}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>"
        ),
    );
    multistatus_response(&xml::multistatus(&resp))
}

// ---- shared response helpers -------------------------------------------

pub fn multistatus_response(body: &str) -> Response {
    (
        StatusCode::MULTI_STATUS,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found\n").into_response()
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "forbidden\n").into_response()
}

fn server_error(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "dav handler error");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response()
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
}

fn header_has(headers: &HeaderMap, name: &str, needle: &str) -> bool {
    header_value(headers, name).as_deref() == Some(needle)
}

/// Loose ETag comparison (ignores weak prefix and surrounding quotes).
fn etag_matches(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.trim()
            .trim_start_matches("W/")
            .trim_matches('"')
            .to_string()
    };
    a == "*" || norm(a) == norm(b)
}

/// Encode a collection's numeric version as an opaque sync-token URI.
pub fn sync_token(n: i64) -> String {
    format!("http://weekling.app/ns/sync/{n}")
}

/// Decode a sync-token back to its numeric version (0 on anything unexpected,
/// which safely forces a full re-sync).
pub fn parse_sync_token(s: &str) -> i64 {
    s.rsplit('/')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Read the Depth header (default 0).
pub fn depth(headers: &HeaderMap) -> u8 {
    match header_value(headers, "depth").as_deref() {
        Some("1") => 1,
        Some("infinity") => 1, // we cap at one level
        _ => 0,
    }
}
