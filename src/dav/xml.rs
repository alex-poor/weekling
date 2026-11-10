//! WebDAV/CalDAV XML: request parsing and response building.
//!
//! Property matching is by local name (namespaces are effectively unique across
//! our supported set), which keeps the parser small and dependency-light.

use std::collections::HashSet;

use quick_xml::Reader;
use quick_xml::events::Event;

/// XML namespace declarations placed on every `<multistatus>` root.
pub const NS: &str = concat!(
    "xmlns:d=\"DAV:\" ",
    "xmlns:cal=\"urn:ietf:params:xml:ns:caldav\" ",
    "xmlns:cs=\"http://calendarserver.org/ns/\" ",
    "xmlns:ical=\"http://apple.com/ns/ical/\" ",
    "xmlns:P=\"https://bitfire.at/webdav-push\""
);

/// Which properties a PROPFIND/REPORT asked for.
#[derive(Clone)]
pub enum PropReq {
    /// `<allprop>` (or `<propname>`): return everything we know.
    All,
    /// An explicit `<prop>` list of local names (lower-cased).
    Names(HashSet<String>),
}

impl PropReq {
    pub fn wants(&self, local: &str) -> bool {
        match self {
            PropReq::All => true,
            PropReq::Names(s) => s.contains(local),
        }
    }
}

/// Local name of a start/empty element, lower-cased, prefix stripped.
fn local_name(e: &quick_xml::events::BytesStart) -> String {
    let full = e.name();
    let bytes = full.as_ref();
    let local = match bytes.iter().position(|&b| b == b':') {
        Some(i) => &bytes[i + 1..],
        None => bytes,
    };
    String::from_utf8_lossy(local).to_ascii_lowercase()
}

/// Parse the requested properties from a PROPFIND (or REPORT) body. An empty or
/// unparseable body is treated as `allprop`, matching common client behaviour.
pub fn parse_prop_request(body: &[u8]) -> PropReq {
    if body.is_empty() {
        return PropReq::All;
    }
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);

    let mut names: HashSet<String> = HashSet::new();
    let mut in_prop = false;
    let mut saw_allprop = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(&e);
                match name.as_str() {
                    "prop" => in_prop = true,
                    "allprop" | "propname" => saw_allprop = true,
                    _ if in_prop => {
                        names.insert(name);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(&e);
                if name == "allprop" || name == "propname" {
                    saw_allprop = true;
                } else if in_prop {
                    names.insert(name);
                }
            }
            Ok(Event::End(e)) => {
                if local_name_end(&e) == "prop" {
                    in_prop = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    if saw_allprop || names.is_empty() {
        PropReq::All
    } else {
        PropReq::Names(names)
    }
}

fn local_name_end(e: &quick_xml::events::BytesEnd) -> String {
    let bytes = e.name();
    let b = bytes.as_ref();
    let local = match b.iter().position(|&c| c == b':') {
        Some(i) => &b[i + 1..],
        None => b,
    };
    String::from_utf8_lossy(local).to_ascii_lowercase()
}

/// Escape text for XML character data / attribute values.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Accumulates found/missing properties for one `<d:response>`.
pub struct PropSet {
    req: PropReq,
    added: HashSet<String>,
    found: String,
}

impl PropSet {
    pub fn new(req: PropReq) -> Self {
        Self {
            req,
            added: HashSet::new(),
            found: String::new(),
        }
    }

    /// Offer a property; its XML is included only if the request wants it.
    pub fn offer(&mut self, local: &str, xml: impl FnOnce() -> String) {
        if self.req.wants(local) {
            self.found.push_str(&xml());
            self.added.insert(local.to_string());
        }
    }

    /// Render the `<d:propstat>` block(s) for this response.
    pub fn render_propstats(self) -> String {
        let mut out = String::new();
        if !self.found.is_empty() {
            out.push_str("<d:propstat><d:prop>");
            out.push_str(&self.found);
            out.push_str("</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>");
        }
        if let PropReq::Names(names) = &self.req {
            let missing: Vec<&String> = names.difference(&self.added).collect();
            if !missing.is_empty() {
                out.push_str("<d:propstat><d:prop>");
                for m in missing {
                    out.push('<');
                    out.push_str(&prefixed(m));
                    out.push_str("/>");
                }
                out.push_str("</d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>");
            }
        }
        out
    }
}

/// Best-effort prefix for a local name (used only in 404 propstats).
fn prefixed(local: &str) -> String {
    let prefix = match local {
        "calendar-home-set"
        | "calendar-description"
        | "supported-calendar-component-set"
        | "calendar-data"
        | "calendar-user-address-set"
        | "calendar-timezone" => "cal",
        "getctag" => "cs",
        "calendar-color" | "calendar-order" => "ical",
        _ => "d",
    };
    format!("{prefix}:{local}")
}

/// Wrap `<d:response>` blocks into a 207 multistatus document.
pub fn multistatus(responses: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus {NS}>{responses}</d:multistatus>"
    )
}

/// A single `<d:response>` with an href and its propstat block(s).
pub fn response(href: &str, propstats: &str) -> String {
    format!(
        "<d:response><d:href>{}</d:href>{}</d:response>",
        escape(href),
        propstats
    )
}

/// Properties pulled from a MKCALENDAR or PROPPATCH body.
#[derive(Default)]
pub struct MkCalProps {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub components: Option<String>,
}

/// Light extraction of the properties we persist from a MKCALENDAR/PROPPATCH
/// body (displayname, description, color, and component set).
pub fn parse_mkcalendar(body: &[u8]) -> MkCalProps {
    let mut props = MkCalProps::default();
    let mut comps: Vec<String> = Vec::new();
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut cur = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = local_name(&e);
                if name == "comp" {
                    for a in e.attributes().flatten() {
                        let key = {
                            let k = a.key;
                            let b = k.as_ref();
                            let l = match b.iter().position(|&c| c == b':') {
                                Some(i) => &b[i + 1..],
                                None => b,
                            };
                            String::from_utf8_lossy(l).to_ascii_lowercase()
                        };
                        if key == "name" {
                            if let Ok(v) = a.normalized_value(quick_xml::XmlVersion::Implicit1_0) {
                                comps.push(v.into_owned());
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
                    "displayname" => props.display_name = Some(txt),
                    "calendar-description" => props.description = Some(txt),
                    "calendar-color" => props.color = Some(txt),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => cur.clear(),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if !comps.is_empty() {
        props.components = Some(comps.join(","));
    }
    props
}

/// A parsed WebDAV-Push `push-register` body (the Web Push fields we store).
#[derive(Default)]
pub struct PushRegistration {
    pub push_resource: String,
    pub p256dh: String,
    pub auth_secret: String,
}

/// Extract the Web Push subscription details from a `push-register` body.
pub fn parse_push_register(body: &[u8]) -> PushRegistration {
    let mut reg = PushRegistration::default();
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut cur = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => cur = local_name(&e),
            Ok(Event::Text(t)) => {
                let txt = t
                    .xml10_content()
                    .map(|v| v.into_owned())
                    .unwrap_or_default();
                match cur.as_str() {
                    "push-resource" => reg.push_resource = txt,
                    "subscription-public-key" => reg.p256dh = txt,
                    "auth-secret" => reg.auth_secret = txt,
                    _ => {}
                }
            }
            Ok(Event::End(_)) => cur.clear(),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_prop_list() {
        let body = br#"<?xml version="1.0"?>
        <d:propfind xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/">
          <d:prop><d:displayname/><cs:getctag/><d:resourcetype/></d:prop>
        </d:propfind>"#;
        match parse_prop_request(body) {
            PropReq::Names(n) => {
                assert!(n.contains("displayname"));
                assert!(n.contains("getctag"));
                assert!(n.contains("resourcetype"));
            }
            PropReq::All => panic!("expected explicit names"),
        }
    }

    #[test]
    fn empty_body_is_allprop() {
        assert!(matches!(parse_prop_request(b""), PropReq::All));
    }

    #[test]
    fn escapes_markup() {
        assert_eq!(escape("a<b&c"), "a&lt;b&amp;c");
    }
}
