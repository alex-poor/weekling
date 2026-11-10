//! PROPFIND: property discovery across principals, calendar homes, calendars,
//! and calendar objects (Depth 0 and 1).

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use super::xml::{PropReq, PropSet, escape, multistatus, response};
use super::{
    Resource, calendar_href, depth, home_href, multistatus_response, object_href, principal_href,
    sync_token,
};
use crate::http::AppState;
use crate::store::{Access, CalObject, Calendar, Principal};

const SUPPORTED_REPORTS: &str = "<d:supported-report-set>\
<d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>\
<d:supported-report><d:report><cal:calendar-query/></d:report></d:supported-report>\
<d:supported-report><d:report><cal:calendar-multiget/></d:report></d:supported-report>\
<d:supported-report><d:report><cal:free-busy-query/></d:report></d:supported-report>\
</d:supported-report-set>";

pub fn handle(
    state: &AppState,
    principal: &Principal,
    resource: &Resource,
    headers: &HeaderMap,
    body: &Bytes,
    access: Access,
) -> Response {
    let req = super::xml::parse_prop_request(body);
    let d = depth(headers);

    let responses = match resource {
        Resource::Root => root_response(principal, &req),
        Resource::Principal { user } => principal_response(principal, user, &req),
        Resource::Home { user } => {
            let mut out = home_response(principal, user, &req);
            if d >= 1 {
                match state.store.list_calendars(user) {
                    Ok(cals) => {
                        for c in cals {
                            out.push_str(&calendar_response(
                                principal,
                                user,
                                &c,
                                &req,
                                &state.push.vapid_public_b64,
                            ));
                        }
                    }
                    Err(e) => return super::server_error(e),
                }
                // Calendars shared *to* this user also appear in their home.
                match state.store.shared_with(user) {
                    Ok(shared) => {
                        for (c, _acc) in shared {
                            let owner = c.owner_id.clone();
                            out.push_str(&calendar_response(
                                principal,
                                &owner,
                                &c,
                                &req,
                                &state.push.vapid_public_b64,
                            ));
                        }
                    }
                    Err(e) => return super::server_error(e),
                }
            }
            out
        }
        Resource::Calendar { user, cal } => {
            let calendar = match state.store.get_calendar(user, cal) {
                Ok(Some(c)) => c,
                Ok(None) => return (StatusCode::NOT_FOUND, "no such calendar\n").into_response(),
                Err(e) => return super::server_error(e),
            };
            let mut out = calendar_response(
                principal,
                user,
                &calendar,
                &req,
                &state.push.vapid_public_b64,
            );
            // Free-busy grantees see the calendar, but not its objects.
            if d >= 1 && access.can_read() {
                match state.store.list_objects(cal, None) {
                    Ok(objs) => {
                        for o in objs {
                            out.push_str(&object_response(user, cal, &o, &req));
                        }
                    }
                    Err(e) => return super::server_error(e),
                }
            }
            out
        }
        Resource::Object { user, cal, name } => match state.store.get_object(cal, name) {
            Ok(Some(o)) => object_response(user, cal, &o, &req),
            Ok(None) => return (StatusCode::NOT_FOUND, "no such object\n").into_response(),
            Err(e) => return super::server_error(e),
        },
        Resource::PushSub { .. } | Resource::Unknown => {
            return (StatusCode::NOT_FOUND, "not found\n").into_response();
        }
    };

    multistatus_response(&multistatus(&responses))
}

fn root_response(principal: &Principal, req: &PropReq) -> String {
    let mut p = PropSet::new(req.clone());
    p.offer("resourcetype", || {
        "<d:resourcetype><d:collection/></d:resourcetype>".into()
    });
    p.offer("displayname", || {
        "<d:displayname>Weekling</d:displayname>".into()
    });
    let cup = principal_href(&principal.id);
    p.offer("current-user-principal", || {
        format!(
            "<d:current-user-principal><d:href>{}</d:href></d:current-user-principal>",
            escape(&cup)
        )
    });
    p.offer("principal-collection-set", || {
        "<d:principal-collection-set><d:href>/dav/principals/</d:href></d:principal-collection-set>"
            .into()
    });
    response("/dav/", &p.render_propstats())
}

fn principal_response(principal: &Principal, user: &str, req: &PropReq) -> String {
    let href = principal_href(user);
    let home = home_href(user);
    let mut p = PropSet::new(req.clone());
    p.offer("resourcetype", || {
        "<d:resourcetype><d:collection/><d:principal/></d:resourcetype>".into()
    });
    let dn = principal.display_name.clone();
    p.offer("displayname", || {
        format!("<d:displayname>{}</d:displayname>", escape(&dn))
    });
    let h1 = href.clone();
    p.offer("current-user-principal", || {
        format!(
            "<d:current-user-principal><d:href>{}</d:href></d:current-user-principal>",
            escape(&h1)
        )
    });
    let h2 = href.clone();
    p.offer("principal-url", || {
        format!(
            "<d:principal-URL><d:href>{}</d:href></d:principal-URL>",
            escape(&h2)
        )
    });
    p.offer("calendar-home-set", || {
        format!(
            "<cal:calendar-home-set><d:href>{}</d:href></cal:calendar-home-set>",
            escape(&home)
        )
    });
    let h3 = href.clone();
    let email = principal.email.clone();
    p.offer("calendar-user-address-set", move || {
        let mut s = format!(
            "<cal:calendar-user-address-set><d:href>{}</d:href>",
            escape(&h3)
        );
        if let Some(e) = &email {
            s.push_str(&format!("<d:href>mailto:{}</d:href>", escape(e)));
        }
        s.push_str("</cal:calendar-user-address-set>");
        s
    });
    response(&href, &p.render_propstats())
}

fn home_response(principal: &Principal, user: &str, req: &PropReq) -> String {
    let href = home_href(user);
    let cup = principal_href(&principal.id);
    let mut p = PropSet::new(req.clone());
    p.offer("resourcetype", || {
        "<d:resourcetype><d:collection/></d:resourcetype>".into()
    });
    p.offer("displayname", || {
        "<d:displayname>Calendars</d:displayname>".into()
    });
    let c1 = cup.clone();
    p.offer("current-user-principal", || {
        format!(
            "<d:current-user-principal><d:href>{}</d:href></d:current-user-principal>",
            escape(&c1)
        )
    });
    p.offer("owner", || {
        format!("<d:owner><d:href>{}</d:href></d:owner>", escape(&cup))
    });
    response(&href, &p.render_propstats())
}

fn calendar_response(
    principal: &Principal,
    user: &str,
    cal: &Calendar,
    req: &PropReq,
    vapid: &str,
) -> String {
    let href = calendar_href(user, &cal.id);
    let cup = principal_href(&principal.id);
    let mut p = PropSet::new(req.clone());
    p.offer("resourcetype", || {
        "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>".into()
    });

    // WebDAV-Push advertisement (draft-bitfire-webdav-push).
    let vapid_key = vapid.to_string();
    p.offer("transports", move || {
        format!(
            "<P:transports><P:web-push><P:vapid-public-key type=\"p256ecdsa\">{}</P:vapid-public-key></P:web-push></P:transports>",
            escape(&vapid_key)
        )
    });
    let topic = cal.id.clone();
    p.offer("topic", move || {
        format!("<P:topic>{}</P:topic>", escape(&topic))
    });
    p.offer("supported-triggers", || {
        "<P:supported-triggers><P:content-update><d:depth>1</d:depth></P:content-update>\
         <P:property-update><d:depth>0</d:depth></P:property-update></P:supported-triggers>"
            .into()
    });
    let dn = cal.display_name.clone();
    p.offer("displayname", || {
        format!("<d:displayname>{}</d:displayname>", escape(&dn))
    });
    if let Some(desc) = &cal.description {
        let d = desc.clone();
        p.offer("calendar-description", || {
            format!(
                "<cal:calendar-description>{}</cal:calendar-description>",
                escape(&d)
            )
        });
    }
    let comps = cal.components.clone();
    p.offer("supported-calendar-component-set", || {
        let inner: String = comps
            .split(',')
            .map(|c| format!("<cal:comp name=\"{}\"/>", escape(c)))
            .collect();
        format!(
            "<cal:supported-calendar-component-set>{inner}</cal:supported-calendar-component-set>"
        )
    });
    if let Some(color) = &cal.color {
        let c = color.clone();
        p.offer("calendar-color", || {
            format!("<ical:calendar-color>{}</ical:calendar-color>", escape(&c))
        });
    }
    let token = cal.sync_token;
    p.offer("getctag", move || {
        format!("<cs:getctag>\"{token}\"</cs:getctag>")
    });
    p.offer("sync-token", move || {
        format!("<d:sync-token>{}</d:sync-token>", sync_token(token))
    });
    p.offer("supported-report-set", || SUPPORTED_REPORTS.into());
    let c1 = cup.clone();
    p.offer("current-user-principal", || {
        format!(
            "<d:current-user-principal><d:href>{}</d:href></d:current-user-principal>",
            escape(&c1)
        )
    });
    p.offer("owner", || {
        format!("<d:owner><d:href>{}</d:href></d:owner>", escape(&cup))
    });
    response(&href, &p.render_propstats())
}

pub fn object_response(user: &str, cal: &str, obj: &CalObject, req: &PropReq) -> String {
    let href = object_href(user, cal, &obj.uid);
    let mut p = PropSet::new(req.clone());
    p.offer("resourcetype", || "<d:resourcetype/>".into());
    let etag = obj.etag.clone();
    p.offer("getetag", move || {
        format!("<d:getetag>{}</d:getetag>", escape(&etag))
    });
    p.offer("getcontenttype", || {
        "<d:getcontenttype>text/calendar; charset=utf-8; component=VEVENT</d:getcontenttype>".into()
    });
    let len = obj.ical_raw.len();
    p.offer("getcontentlength", move || {
        format!("<d:getcontentlength>{len}</d:getcontentlength>")
    });
    let raw = obj.ical_raw.clone();
    p.offer("calendar-data", move || {
        format!("<cal:calendar-data>{}</cal:calendar-data>", escape(&raw))
    });
    response(&href, &p.render_propstats())
}
