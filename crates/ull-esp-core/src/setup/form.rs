//! Reads the setup form: `application/x-www-form-urlencoded` fields.

use crate::settings::Text;

/// Longest decoded field; longer input is refused, not cut.
pub const FIELD_MAX: usize = 128;

/// The decoded value of field `name`; empty when absent, `None` when it is
/// too long or not valid UTF-8.
pub fn field(body: &[u8], name: &str) -> Option<Text<FIELD_MAX>> {
    let raw = body
        .split(|byte| *byte == b'&')
        .find_map(|pair| {
            let (key, value) = match pair.iter().position(|byte| *byte == b'=') {
                Some(eq) => (&pair[..eq], &pair[eq + 1..]),
                None => (pair, &pair[pair.len()..]),
            };
            (key == name.as_bytes()).then_some(value)
        })
        .unwrap_or_default();
    decode(raw)
}

fn decode(raw: &[u8]) -> Option<Text<FIELD_MAX>> {
    let mut bytes = [0u8; FIELD_MAX];
    let mut len = 0;
    let mut index = 0;
    while index < raw.len() {
        let byte = match raw[index] {
            b'+' => b' ',
            b'%' => {
                let hex = raw.get(index + 1..index + 3)?;
                index += 2;
                (hex_digit(hex[0])? << 4) | hex_digit(hex[1])?
            }
            byte => byte,
        };
        *bytes.get_mut(len)? = byte;
        len += 1;
        index += 1;
    }
    Text::new(core::str::from_utf8(&bytes[..len]).ok()?)
}

fn hex_digit(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|digit| digit as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_fields() {
        let body = b"ssid=Caf%C3%A9+Wi-Fi&password=a%26b%3Dc&server=";
        assert_eq!(field(body, "ssid").unwrap().as_str(), "Café Wi-Fi");
        assert_eq!(field(body, "password").unwrap().as_str(), "a&b=c");
        assert_eq!(field(body, "server").unwrap().as_str(), "");
        assert_eq!(field(body, "missing").unwrap().as_str(), "");
    }

    #[test]
    fn refuses_bad_input() {
        assert!(field(b"ssid=%zz", "ssid").is_none());
        assert!(field(b"ssid=%c3", "ssid").is_none());
        let long = [b'a'; FIELD_MAX + 1];
        let mut body = b"ssid=".to_vec();
        body.extend_from_slice(&long);
        assert!(field(&body, "ssid").is_none());
    }
}
