//! Minimal, dependency-free iCalendar (RFC 5545) field extraction.
//!
//! We store every object's bytes verbatim for perfect client round-tripping and
//! only pull out the handful of fields we index on (UID, DTSTART/DTEND, SUMMARY,
//! STATUS, RRULE presence). This is deliberately not a full parser.

/// Fields extracted from the primary VEVENT/VTODO of a calendar object.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct IcalFields {
    pub component: Option<String>, // "VEVENT" | "VTODO"
    pub uid: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub dtstart: Option<String>, // normalized 'YYYYMMDDTHHMMSS' (Z dropped)
    pub dtend: Option<String>,
    pub has_rrule: bool,
}

/// Unfold RFC 5545 line continuations (a line beginning with SPACE or TAB is a
/// continuation of the previous one).
fn unfold(input: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in input.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines
}

/// Split a content line into (property name upper-cased, raw value after the
/// first unquoted colon).
fn split_prop(line: &str) -> Option<(String, &str)> {
    let colon = line.find(':')?;
    let (head, value) = line.split_at(colon);
    let value = &value[1..];
    let name = head.split(';').next().unwrap_or(head);
    Some((name.to_ascii_uppercase(), value))
}

/// Normalize a date/time value to a sortable `YYYYMMDDTHHMMSS` string (trailing
/// `Z` dropped; a date-only value is padded to midnight).
pub fn normalize_dt(value: &str) -> Option<String> {
    let v: String = value
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'T')
        .collect();
    match v.len() {
        8 => Some(format!("{v}T000000")), // DATE
        15 => Some(v),                    // DATE-TIME (floating or had Z)
        _ => None,
    }
}

/// Extract indexed fields from the first top-level VEVENT or VTODO.
pub fn extract(ical: &str) -> IcalFields {
    let mut f = IcalFields::default();
    let mut inside = false;
    let mut depth = 0u32; // nesting below the component (e.g. VALARM)

    for line in unfold(ical) {
        if let Some(rest) = line.strip_prefix("BEGIN:") {
            let comp = rest.to_ascii_uppercase();
            if !inside && (comp == "VEVENT" || comp == "VTODO") {
                inside = true;
                f.component = Some(comp);
            } else if inside {
                depth += 1;
            }
            continue;
        }
        if line.starts_with("END:") {
            if inside && depth == 0 {
                break;
            } else if inside {
                depth -= 1;
            }
            continue;
        }
        if !inside || depth > 0 {
            continue;
        }
        let Some((name, value)) = split_prop(&line) else {
            continue;
        };
        match name.as_str() {
            "UID" => f.uid = Some(value.to_string()),
            "SUMMARY" => f.summary = Some(unescape_text(value)),
            "STATUS" => f.status = Some(value.to_ascii_uppercase()),
            "DTSTART" => f.dtstart = normalize_dt(value),
            "DTEND" => f.dtend = normalize_dt(value),
            "RRULE" => f.has_rrule = true,
            _ => {}
        }
    }
    f
}

/// Validate just enough to reject junk at PUT time: a VCALENDAR wrapper and a
/// component carrying a UID.
pub fn validate(ical: &str) -> Result<IcalFields, String> {
    if !ical.to_ascii_uppercase().contains("BEGIN:VCALENDAR") {
        return Err("missing BEGIN:VCALENDAR".into());
    }
    let f = extract(ical);
    if f.component.is_none() {
        return Err("no VEVENT or VTODO component".into());
    }
    if f.uid.as_deref().unwrap_or("").is_empty() {
        return Err("component has no UID".into());
    }
    Ok(f)
}

fn unescape_text(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:abc-123\r\nSUMMARY:Team sync\r\nDTSTART:20260115T090000Z\r\nDTEND:20260115T100000Z\r\nSTATUS:confirmed\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nDTSTART:19990101T000000Z\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn extracts_primary_event_not_alarm() {
        let f = extract(SAMPLE);
        assert_eq!(f.component.as_deref(), Some("VEVENT"));
        assert_eq!(f.uid.as_deref(), Some("abc-123"));
        assert_eq!(f.summary.as_deref(), Some("Team sync"));
        assert_eq!(f.status.as_deref(), Some("CONFIRMED"));
        assert_eq!(f.dtstart.as_deref(), Some("20260115T090000")); // not the VALARM's
        assert!(!f.has_rrule);
    }

    #[test]
    fn date_only_is_padded() {
        assert_eq!(normalize_dt("20260115").as_deref(), Some("20260115T000000"));
    }

    #[test]
    fn validate_rejects_junk() {
        assert!(validate("not a calendar").is_err());
        assert!(validate(SAMPLE).is_ok());
    }
}
