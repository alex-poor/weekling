//! REPORT: calendar-query (with time-range), calendar-multiget, and
//! sync-collection (RFC 6578).

use axum::body::Bytes;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use quick_xml::Reader;
use quick_xml::events::Event;

use super::xml::{escape, multistatus, parse_prop_request, response};
use super::{
    Resource, multistatus_response, object_href, parse_path, parse_sync_token, sync_token,
};
use crate::http::AppState;
use crate::ical;
use crate::store::{Access, Principal};

#[derive(Default)]
struct ReportReq {
    kind: String,
    time_range: Option<(String, String)>,
    component: Option<String>, // VEVENT / VTODO from the innermost comp-filter
    hrefs: Vec<String>,
    sync_token: Option<String>,
}

fn local(bytes: &[u8]) -> String {
    let l = match bytes.iter().position(|&b| b == b':') {
        Some(i) => &bytes[i + 1..],
        None => bytes,
    };
    String::from_utf8_lossy(l).to_ascii_lowercase()
}

fn parse_report(body: &[u8]) -> ReportReq {
    let mut r = ReportReq::default();
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut cur = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = local(e.name().as_ref());
                if r.kind.is_empty() {
                    r.kind = name.clone();
                }
                if name == "time-range" {
                    let mut start = None;
                    let mut end = None;
                    for a in e.attributes().flatten() {
                        let key = local(a.key.as_ref());
                        let val = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        match key.as_str() {
                            "start" => start = ical::normalize_dt(&val),
                            "end" => end = ical::normalize_dt(&val),
                            _ => {}
                        }
                    }
                    if let (Some(s), Some(en)) = (start, end) {
                        r.time_range = Some((s, en));
                    }
                }
                if name == "comp-filter" {
                    for a in e.attributes().flatten() {
                        if local(a.key.as_ref()) == "name" {
                            let v = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map(|v| v.into_owned().to_ascii_uppercase())
                                .unwrap_or_default();
                            if v == "VEVENT" || v == "VTODO" {
                                r.component = Some(v);
                            }
                        }
                    }
                }
                cur = name;
            }
            Ok(Event::Text(t)) => {
                let txt = t
                    .xml10_content()
                    .map(|v| v.into_owned())
                    .unwrap_or_default();
                match cur.as_str() {
                    "href" => r.hrefs.push(txt),
                    "sync-token" if !txt.is_empty() => r.sync_token = Some(txt),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => cur.clear(),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    r
}

pub fn handle(
    state: &AppState,
    _principal: &Principal,
    resource: &Resource,
    body: &Bytes,
    access: Access,
) -> Response {
    let report = parse_report(body);
    let prop_req = parse_prop_request(body);

    let (user, cal) = match resource {
        Resource::Calendar { user, cal } => (user.clone(), cal.clone()),
        _ => return (StatusCode::FORBIDDEN, "report not supported here\n").into_response(),
    };

    // Free-busy grantees may only run free-busy-query.
    if !access.can_read() && report.kind != "free-busy-query" {
        return (StatusCode::FORBIDDEN, "only free-busy-query is permitted\n").into_response();
    }

    let component = report.component.as_deref();

    match report.kind.as_str() {
        "calendar-query" => {
            let objs = match &report.time_range {
                Some((s, e)) => state.store.list_objects_in_range(&cal, s, e, component),
                None => state.store.list_objects(&cal, component),
            };
            match objs {
                Ok(objs) => {
                    let mut out = String::new();
                    for o in objs {
                        out.push_str(&super::propfind::object_response(
                            &user, &cal, &o, &prop_req,
                        ));
                    }
                    multistatus_response(&multistatus(&out))
                }
                Err(e) => super::server_error(e),
            }
        }
        "calendar-multiget" => {
            let mut out = String::new();
            for href in &report.hrefs {
                if let Resource::Object { name, .. } = parse_path(href) {
                    match state.store.get_object(&cal, &name) {
                        Ok(Some(o)) => out.push_str(&super::propfind::object_response(
                            &user, &cal, &o, &prop_req,
                        )),
                        Ok(None) => out.push_str(&response(
                            href,
                            "<d:status>HTTP/1.1 404 Not Found</d:status>",
                        )),
                        Err(e) => return super::server_error(e),
                    }
                }
            }
            multistatus_response(&multistatus(&out))
        }
        "sync-collection" => {
            let since = report
                .sync_token
                .as_deref()
                .map(parse_sync_token)
                .unwrap_or(0);
            match state.store.changes_since(&cal, since) {
                Ok((changes, token)) => {
                    let mut out = String::new();
                    for c in changes {
                        let href = object_href(&user, &cal, &c.uid);
                        if c.deleted {
                            out.push_str(&response(
                                &href,
                                "<d:status>HTTP/1.1 404 Not Found</d:status>",
                            ));
                        } else {
                            let etag = c.etag.unwrap_or_default();
                            out.push_str(&response(
                                &href,
                                &format!(
                                    "<d:propstat><d:prop><d:getetag>{}</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>",
                                    escape(&etag)
                                ),
                            ));
                        }
                    }
                    let body = format!(
                        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus {}>{}<d:sync-token>{}</d:sync-token></d:multistatus>",
                        super::xml::NS,
                        out,
                        sync_token(token)
                    );
                    multistatus_response(&body)
                }
                Err(e) => super::server_error(e),
            }
        }
        "free-busy-query" => {
            let Some((start, end)) = report.time_range.clone() else {
                return (
                    StatusCode::BAD_REQUEST,
                    "free-busy-query needs a time-range\n",
                )
                    .into_response();
            };
            match state
                .store
                .list_objects_in_range(&cal, &start, &end, Some("VEVENT"))
            {
                Ok(objs) => {
                    let mut periods = String::new();
                    for o in objs {
                        if let (Some(s), Some(e)) = (&o.dtstart, &o.dtend) {
                            periods.push_str(&format!("FREEBUSY;FBTYPE=BUSY:{}Z/{}Z\r\n", s, e));
                        }
                    }
                    let ics = format!(
                        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//weekling//caldav//EN\r\n\
                         BEGIN:VFREEBUSY\r\nDTSTART:{start}Z\r\nDTEND:{end}Z\r\n{periods}\
                         END:VFREEBUSY\r\nEND:VCALENDAR\r\n"
                    );
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
                        ics,
                    )
                        .into_response()
                }
                Err(e) => super::server_error(e),
            }
        }
        other => (
            StatusCode::BAD_REQUEST,
            format!("unsupported report: {other}\n"),
        )
            .into_response(),
    }
}
