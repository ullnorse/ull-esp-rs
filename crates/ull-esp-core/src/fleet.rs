//! The ull-fleet device protocol (`/api/v1`): device ids, the check-in JSON
//! body and its reply. See `docs/DESIGN.md` in ull-fleet-rs.

use core::fmt::{self, Write};

/// Most metrics the server accepts in one check-in.
pub const MAX_METRICS: usize = 16;

/// A device id: the Wi-Fi MAC address as 12 lowercase hex digits.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DeviceId([u8; 12]);

impl DeviceId {
    pub fn from_mac(mac: [u8; 6]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut id = [0u8; 12];
        for (index, byte) in mac.iter().enumerate() {
            id[index * 2] = HEX[usize::from(byte >> 4)];
            id[index * 2 + 1] = HEX[usize::from(byte & 0x0f)];
        }
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII hex digits are ever stored.
        core::str::from_utf8(&self.0).unwrap_or("")
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One check-in. Strings are escaped when written; metrics that are not
/// finite are left out.
#[derive(Clone, Debug)]
pub struct Checkin<'a> {
    pub project: &'a str,
    pub version: &'a str,
    pub secure_version: u32,
    pub partition: &'a str,
    pub ota_state: &'a str,
    pub uptime_s: u64,
    pub rssi_dbm: Option<i32>,
    pub ip: Option<[u8; 4]>,
    pub last_ota_error: Option<&'a str>,
    /// `(name, value)`; names carry their unit, e.g. `temperature_c`.
    pub metrics: &'a [(&'a str, f32)],
}

impl Checkin<'_> {
    pub fn write_json(&self, out: &mut impl Write) -> fmt::Result {
        out.write_str("{\"project\":")?;
        write_string(out, self.project)?;
        out.write_str(",\"version\":")?;
        write_string(out, self.version)?;
        write!(out, ",\"secure_version\":{}", self.secure_version)?;
        out.write_str(",\"partition\":")?;
        write_string(out, self.partition)?;
        out.write_str(",\"ota_state\":")?;
        write_string(out, self.ota_state)?;
        write!(out, ",\"uptime_s\":{}", self.uptime_s)?;
        if let Some(rssi) = self.rssi_dbm {
            write!(out, ",\"rssi_dbm\":{rssi}")?;
        }
        if let Some([a, b, c, d]) = self.ip {
            write!(out, ",\"ip\":\"{a}.{b}.{c}.{d}\"")?;
        }
        if let Some(error) = self.last_ota_error {
            out.write_str(",\"last_ota_error\":")?;
            write_string(out, error)?;
        }
        out.write_str(",\"metrics\":{")?;
        let mut first = true;
        for (name, value) in self.metrics.iter().take(MAX_METRICS) {
            if !value.is_finite() {
                continue;
            }
            if !first {
                out.write_char(',')?;
            }
            first = false;
            write_string(out, name)?;
            write!(out, ":{value}")?;
        }
        out.write_str("}}")
    }
}

/// Writes `value` as a JSON string, truncated to 200 bytes at a char boundary.
fn write_string(out: &mut impl Write, value: &str) -> fmt::Result {
    out.write_char('"')?;
    let mut written = 0;
    for ch in value.chars() {
        written += ch.len_utf8();
        if written > 200 {
            break;
        }
        match ch {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            ch if (ch as u32) < 0x20 => write!(out, "\\u{:04x}", ch as u32)?,
            ch => out.write_char(ch)?,
        }
    }
    out.write_char('"')
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CheckinReply {
    /// The server has a release for this device; fetch `/firmware`.
    pub update: bool,
    /// How long to wait before the next check-in.
    pub poll_interval_s: Option<u32>,
}

/// Parses `{"update":true,"poll_interval_s":30}`. Unknown fields are ignored;
/// a missing or malformed `update` field is an error.
pub fn parse_reply(body: &[u8]) -> Option<CheckinReply> {
    let body = core::str::from_utf8(body).ok()?;
    let update = match field(body, "update")? {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    let poll_interval_s = field(body, "poll_interval_s")
        .and_then(|value| value.parse().ok())
        .filter(|seconds| *seconds > 0);
    Some(CheckinReply {
        update,
        poll_interval_s,
    })
}

/// The raw token after `"key":` in a flat JSON object.
fn field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = body;
    loop {
        let start = rest.find('"')?;
        rest = &rest[start + 1..];
        let end = rest.find('"')?;
        let name = &rest[..end];
        rest = rest[end + 1..].trim_start();
        let Some(after_colon) = rest.strip_prefix(':') else {
            continue;
        };
        let value = after_colon.trim_start();
        if name == key {
            let end = value
                .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
                .unwrap_or(value.len());
            return Some(&value[..end]);
        }
        rest = value;
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::String;

    use super::*;

    #[test]
    fn device_id_from_mac() {
        let id = DeviceId::from_mac([0x24, 0x6f, 0x28, 0xa1, 0xb2, 0xc4]);
        assert_eq!(id.as_str(), "246f28a1b2c4");
    }

    #[test]
    fn writes_checkin_json() {
        let checkin = Checkin {
            project: "sht3x-ssd1306",
            version: "0.1.0+abc-1",
            secure_version: 1,
            partition: "ota_0",
            ota_state: "valid",
            uptime_s: 42,
            rssi_dbm: Some(-61),
            ip: Some([192, 168, 1, 20]),
            last_ota_error: Some("bad \"sig\"\n"),
            metrics: &[
                ("temperature_c", 26.25),
                ("broken", f32::NAN),
                ("humidity_pct", 52.5),
            ],
        };
        let mut out = String::new();
        checkin.write_json(&mut out).unwrap();
        assert_eq!(
            out,
            r#"{"project":"sht3x-ssd1306","version":"0.1.0+abc-1","secure_version":1,"partition":"ota_0","ota_state":"valid","uptime_s":42,"rssi_dbm":-61,"ip":"192.168.1.20","last_ota_error":"bad \"sig\"\u000a","metrics":{"temperature_c":26.25,"humidity_pct":52.5}}"#
        );
    }

    #[test]
    fn minimal_checkin_json() {
        let checkin = Checkin {
            project: "p",
            version: "1",
            secure_version: 0,
            partition: "",
            ota_state: "",
            uptime_s: 0,
            rssi_dbm: None,
            ip: None,
            last_ota_error: None,
            metrics: &[],
        };
        let mut out = String::new();
        checkin.write_json(&mut out).unwrap();
        assert!(out.ends_with(r#""uptime_s":0,"metrics":{}}"#), "{out}");
    }

    #[test]
    fn parses_reply() {
        assert_eq!(
            parse_reply(br#"{"update":true,"poll_interval_s":30}"#),
            Some(CheckinReply {
                update: true,
                poll_interval_s: Some(30)
            })
        );
        assert_eq!(
            parse_reply(b"{ \"poll_interval_s\" : 5 , \"update\" : false }"),
            Some(CheckinReply {
                update: false,
                poll_interval_s: Some(5)
            })
        );
        assert_eq!(
            parse_reply(br#"{"update":false}"#).unwrap().poll_interval_s,
            None
        );
        assert_eq!(parse_reply(br#"{"update":"yes"}"#), None);
        assert_eq!(parse_reply(br#"{"other":1}"#), None);
        assert_eq!(parse_reply(b"not json"), None);
        // A key inside a string value is not a field.
        assert_eq!(
            parse_reply(br#"{"note":"update","update":true}"#).map(|r| r.update),
            Some(true)
        );
    }
}
