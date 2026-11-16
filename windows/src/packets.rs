//! The JSON packets of `docs/features.md` this app knows, sections 2 to 4.
//!
//! Parsing is tolerant, as the contract asks: members this app does not
//! know are ignored, a packet of a type it does not know is
//! [`Parsed::Unknown`], and only a packet that is not a JSON object with a
//! string `type`, or a known type with members of the wrong kind, is
//! [`Parsed::Invalid`].

use serde::{Deserialize, Serialize};

/// The `platform` this app announces.
pub const PLATFORM: &str = "windows";

/// The largest file a transfer may carry: 4 GiB.
pub const MAX_FILE_SIZE: u64 = 4 << 30;

/// The longest URL a `share.url` may carry, in bytes.
pub const MAX_URL_LEN: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Packet {
    #[serde(rename = "capabilities")]
    Capabilities(Capabilities),
    #[serde(rename = "state.request")]
    StateRequest {},
    #[serde(rename = "battery")]
    Battery(Battery),
    #[serde(rename = "connectivity")]
    Connectivity(Connectivity),
    #[serde(rename = "ring")]
    Ring {},
    #[serde(rename = "ring.stop")]
    RingStop {},
    #[serde(rename = "ping")]
    Ping {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "share.url")]
    ShareUrl { url: String },
    #[serde(rename = "file.offer")]
    FileOffer(FileOffer),
    #[serde(rename = "file.accept")]
    FileAccept { id: String, offset: u64 },
    #[serde(rename = "file.reject")]
    FileReject { id: String, reason: String },
    #[serde(rename = "file.ack")]
    FileAck { id: String, offset: u64 },
    #[serde(rename = "file.done")]
    FileDone { id: String },
    #[serde(rename = "file.cancel")]
    FileCancel { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub platform: String,
    #[serde(default)]
    pub incoming: Vec<String>,
    #[serde(default)]
    pub outgoing: Vec<String>,
}

impl Capabilities {
    /// Whether the device acts on packets of type `kind`.
    pub fn handles(&self, kind: &str) -> bool {
        self.incoming.iter().any(|known| known == kind)
    }

    /// Whether the device may send packets of type `kind`.
    pub fn sends(&self, kind: &str) -> bool {
        self.outgoing.iter().any(|known| known == kind)
    }
}

/// A missing member means "unknown".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charging: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connectivity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOffer {
    pub id: String,
    pub name: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
}

/// Why a receiver refuses a file, as `file.reject` puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    Disabled,
    TooLarge,
    NoSpace,
    Busy,
}

impl RejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::Disabled => "disabled",
            RejectReason::TooLarge => "tooLarge",
            RejectReason::NoSpace => "noSpace",
            RejectReason::Busy => "busy",
        }
    }

    /// `None` for a reason this app does not know.
    pub fn parse(reason: &str) -> Option<RejectReason> {
        [
            RejectReason::Disabled,
            RejectReason::TooLarge,
            RejectReason::NoSpace,
            RejectReason::Busy,
        ]
        .into_iter()
        .find(|known| known.as_str() == reason)
    }
}

/// What a received packet turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Known(Packet),
    /// A well-formed packet of a type this app does not know, ignored.
    Unknown(String),
    /// Not a JSON object with a string `type`, or a known type with members
    /// of the wrong kind: dropped.
    Invalid,
}

/// The types [`Packet`] covers, for telling an unknown type from a known
/// type with bad members.
const KNOWN: &[&str] = &[
    "capabilities",
    "state.request",
    "battery",
    "connectivity",
    "ring",
    "ring.stop",
    "ping",
    "share.url",
    "file.offer",
    "file.accept",
    "file.reject",
    "file.ack",
    "file.done",
    "file.cancel",
];

pub fn parse(json: &str) -> Parsed {
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str(json) else {
        return Parsed::Invalid;
    };
    let Some(kind) = object.get("type").and_then(|kind| kind.as_str()) else {
        return Parsed::Invalid;
    };
    if !KNOWN.contains(&kind) {
        return Parsed::Unknown(kind.to_owned());
    }
    match serde_json::from_value(serde_json::Value::Object(object)) {
        Ok(packet) => Parsed::Known(packet),
        Err(_) => Parsed::Invalid,
    }
}

impl Packet {
    pub fn to_json(&self) -> String {
        // A packet of strings, numbers and lists always serialises.
        serde_json::to_string(self).expect("a packet serialises")
    }
}

/// A transfer id as packets write it: 32 lowercase hex characters.
pub fn transfer_id(id: &[u8; 16]) -> String {
    weft_cli::hex(id)
}

/// The inverse of [`transfer_id`]; uppercase is accepted.
pub fn parse_transfer_id(text: &str) -> Option<[u8; 16]> {
    weft_cli::unhex(text)
}

/// Whether `url` may be opened from a `share.url`: http or https, at most
/// [`MAX_URL_LEN`] bytes, no whitespace or control characters, and a host.
pub fn is_shareable_url(url: &str) -> bool {
    if url.is_empty() || url.len() > MAX_URL_LEN {
        return false;
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let rest = match lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    {
        Some(rest) => rest,
        None => return false,
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    !host.is_empty() && !host.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_packets_parse_with_unknown_members_ignored() {
        assert_eq!(
            parse(r#"{"type":"battery","level":82,"charging":true,"low":false,"extra":1}"#),
            Parsed::Known(Packet::Battery(Battery {
                level: Some(82),
                charging: Some(true),
                low: Some(false),
            }))
        );
        // Missing members are unknown, not zero.
        assert_eq!(
            parse(r#"{"type":"battery"}"#),
            Parsed::Known(Packet::Battery(Battery::default()))
        );
        assert_eq!(
            parse(r#"{"type":"connectivity","network":"LTE","signal":3}"#),
            Parsed::Known(Packet::Connectivity(Connectivity {
                network: Some("LTE".into()),
                signal: Some(3),
            }))
        );
        assert_eq!(
            parse(r#"{"type":"ring","why":"lost"}"#),
            Parsed::Known(Packet::Ring {})
        );
        assert_eq!(
            parse(r#"{"type":"state.request"}"#),
            Parsed::Known(Packet::StateRequest {})
        );
        assert_eq!(
            parse(r#"{"type":"ping"}"#),
            Parsed::Known(Packet::Ping { message: None })
        );
        assert_eq!(
            parse(r#"{"type":"file.offer","id":"00","name":"a.txt","size":3}"#),
            Parsed::Known(Packet::FileOffer(FileOffer {
                id: "00".into(),
                name: "a.txt".into(),
                size: 3,
                mime: None,
            }))
        );
        assert_eq!(
            parse(
                r#"{"type":"capabilities","platform":"android","incoming":["ring"],"outgoing":["battery"],"version":2}"#
            ),
            Parsed::Known(Packet::Capabilities(Capabilities {
                platform: "android".into(),
                incoming: vec!["ring".into()],
                outgoing: vec!["battery".into()],
            }))
        );
    }

    #[test]
    fn unknown_and_invalid_packets() {
        assert_eq!(
            parse(r#"{"type":"media","players":[]}"#),
            Parsed::Unknown("media".into())
        );
        for invalid in [
            "",
            "[]",
            "\"ring\"",
            "{}",
            r#"{"type":7}"#,
            r#"{"type":"battery","level":"full"}"#,
            r#"{"type":"battery","level":300}"#,
            r#"{"type":"file.accept","id":"00"}"#,
            r#"{"type":"share.url"}"#,
            "{not json",
        ] {
            assert_eq!(parse(invalid), Parsed::Invalid, "{invalid}");
        }
    }

    #[test]
    fn packets_serialise_as_the_contract_writes_them() {
        assert_eq!(
            Packet::StateRequest {}.to_json(),
            r#"{"type":"state.request"}"#
        );
        assert_eq!(Packet::Ring {}.to_json(), r#"{"type":"ring"}"#);
        assert_eq!(Packet::RingStop {}.to_json(), r#"{"type":"ring.stop"}"#);
        assert_eq!(
            Packet::FileAck {
                id: "ab".into(),
                offset: 131072
            }
            .to_json(),
            r#"{"type":"file.ack","id":"ab","offset":131072}"#
        );
        assert_eq!(
            Packet::FileOffer(FileOffer {
                id: "ab".into(),
                name: "a.txt".into(),
                size: 0,
                mime: None
            })
            .to_json(),
            r#"{"type":"file.offer","id":"ab","name":"a.txt","size":0}"#
        );
        let capabilities = Packet::Capabilities(Capabilities {
            platform: PLATFORM.into(),
            incoming: vec!["ring".into()],
            outgoing: vec![],
        });
        assert_eq!(parse(&capabilities.to_json()), Parsed::Known(capabilities));
    }

    #[test]
    fn transfer_ids_round_trip() {
        let id = [0xab; 16];
        let text = transfer_id(&id);
        assert_eq!(text, "ab".repeat(16));
        assert_eq!(parse_transfer_id(&text), Some(id));
        assert_eq!(parse_transfer_id(&text.to_uppercase()), Some(id));
        assert_eq!(parse_transfer_id("ab"), None);
        assert_eq!(parse_transfer_id(&"zz".repeat(16)), None);
        assert_eq!(parse_transfer_id(&"é".repeat(16)), None);
    }

    #[test]
    fn reject_reasons() {
        for reason in ["disabled", "tooLarge", "noSpace", "busy"] {
            assert_eq!(RejectReason::parse(reason).unwrap().as_str(), reason);
        }
        assert_eq!(RejectReason::parse("other"), None);
    }

    #[test]
    fn only_web_links_are_shareable() {
        assert!(is_shareable_url("https://example.com/a?b=c#d"));
        assert!(is_shareable_url("HTTP://example.com"));
        assert!(is_shareable_url("https://user@example.com:8080/"));
        for refused in [
            "",
            "file:///C:/Windows/system32/calc.exe",
            "javascript:alert(1)",
            "ms-settings:privacy",
            "https://",
            "https:///path",
            "https://exa mple.com",
            "https://example.com/\nline",
            "ftp://example.com",
        ] {
            assert!(!is_shareable_url(refused), "{refused}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert!(!is_shareable_url(&long));
    }
}
