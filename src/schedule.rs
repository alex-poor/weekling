//! RFC 6638 implicit scheduling between *local* calendar users.
//!
//! When an organizer writes/removes an event with attendees, we deliver iTIP
//! REQUEST/CANCEL messages into each local attendee's scheduling inbox; when an
//! attendee changes their participation status, we deliver a REPLY to the
//! organizer's inbox. iMIP (email delivery to remote users) is out of scope.

use crate::http::AppState;
use crate::ical::{self, IcalFields};
use crate::store::{NewObject, inbox_id};
use crate::util;

/// Resolve a calendar-user-address (`mailto:` or a principal URL) to a local id.
fn resolve(state: &AppState, addr: &str) -> Option<String> {
    let a = addr.trim();
    if let Some(email) = a.to_ascii_lowercase().strip_prefix("mailto:")
        && let Ok(Some(id)) = state.store.principal_by_email(email)
    {
        return Some(id);
    }
    if let Some(idx) = a.find("/dav/principals/") {
        let rest = &a[idx + "/dav/principals/".len()..];
        let user = rest.trim_end_matches('/').split('/').next().unwrap_or("");
        if !user.is_empty() && matches!(state.store.get_principal(user), Ok(Some(_))) {
            return Some(user.to_string());
        }
    }
    None
}

/// Organizer wrote/updated an event → REQUEST to attendees; or an attendee
/// updated their copy → REPLY to the organizer.
pub fn on_put(state: &AppState, actor: &str, ics: &str) {
    let f = ical::extract(ics);
    let (Some(organizer), false) = (f.organizer.as_deref(), f.attendees.is_empty()) else {
        return; // not a scheduling object
    };
    let organizer_id = resolve(state, organizer);

    if organizer_id.as_deref() == Some(actor) {
        let msg = with_method(ics, "REQUEST");
        for att in &f.attendees {
            if let Some(att_id) = resolve(state, &att.value)
                && att_id != actor
            {
                deliver(state, &att_id, &msg);
            }
        }
    } else if let Some(org_id) = organizer_id {
        let is_attendee = f
            .attendees
            .iter()
            .any(|a| resolve(state, &a.value).as_deref() == Some(actor));
        if is_attendee && org_id != actor {
            deliver(state, &org_id, &build_reply(state, &f, actor));
        }
    }
}

/// Organizer removed an event → CANCEL to attendees.
pub fn on_delete(state: &AppState, actor: &str, ics: &str) {
    let f = ical::extract(ics);
    let Some(organizer) = f.organizer.as_deref() else {
        return;
    };
    if f.attendees.is_empty() || resolve(state, organizer).as_deref() != Some(actor) {
        return;
    }
    let msg = with_method(ics, "CANCEL");
    for att in &f.attendees {
        if let Some(att_id) = resolve(state, &att.value)
            && att_id != actor
        {
            deliver(state, &att_id, &msg);
        }
    }
}

fn deliver(state: &AppState, principal_id: &str, ics: &str) {
    if state.store.ensure_inbox(principal_id).is_err() {
        return;
    }
    let f = ical::extract(ics);
    let uid = util::new_id();
    let etag = util::etag_for(ics.as_bytes());
    let obj = NewObject {
        collection_id: &inbox_id(principal_id),
        uid: &uid,
        etag: &etag,
        ical_raw: ics,
        dtstart: f.dtstart.as_deref(),
        dtend: f.dtend.as_deref(),
        summary: f.summary.as_deref(),
        status: f.status.as_deref(),
        component: f.component.as_deref(),
        has_rrule: f.has_rrule,
    };
    match state.store.put_object(&obj, &util::new_id()) {
        Ok(_) => tracing::info!(to = principal_id, "schedule: delivered iTIP to inbox"),
        Err(e) => tracing::warn!(error = %e, to = principal_id, "schedule: inbox delivery failed"),
    }
}

/// Insert/replace a VCALENDAR-level METHOD, keeping the rest of the object.
fn with_method(ics: &str, method: &str) -> String {
    let mut out = String::new();
    for line in ics.split_inclusive('\n') {
        let trimmed = line.trim_end();
        if trimmed.eq_ignore_ascii_case("BEGIN:VCALENDAR") {
            out.push_str(line);
            out.push_str(&format!("METHOD:{method}\r\n"));
        } else if !trimmed.to_ascii_uppercase().starts_with("METHOD:") {
            out.push_str(line);
        }
    }
    out
}

/// A minimal METHOD:REPLY carrying just the replying attendee's PARTSTAT.
fn build_reply(state: &AppState, f: &IcalFields, actor: &str) -> String {
    let me = f
        .attendees
        .iter()
        .find(|a| resolve(state, &a.value).as_deref() == Some(actor));
    let value = me.map(|a| a.value.clone()).unwrap_or_default();
    let partstat = me
        .and_then(|a| a.partstat.clone())
        .unwrap_or_else(|| "NEEDS-ACTION".to_string());
    let uid = f.uid.clone().unwrap_or_default();
    let organizer = f.organizer.clone().unwrap_or_default();
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//weekling//caldav//EN\r\nMETHOD:REPLY\r\n\
         BEGIN:VEVENT\r\nUID:{uid}\r\nORGANIZER:{organizer}\r\n\
         ATTENDEE;PARTSTAT={partstat}:{value}\r\nDTSTAMP:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        now_utc()
    )
}

/// Current UTC time as an iCalendar DATE-TIME (`YYYYMMDDTHHMMSSZ`).
fn now_utc() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}
