//! Device settings kept in flash: Wi-Fi name and password, fleet server URL.
//!
//! The settings live in two 4 KiB sectors. Each save writes a complete
//! [record](encode) into the sector that does not hold the current one, with
//! a sequence number one higher. A power loss during the save leaves the old
//! record intact, and the new one fails its checksum and is ignored. On boot
//! [`newest`] picks the valid record with the highest sequence number.
//!
//! Record layout, little endian:
//!
//! ```text
//! magic u32 | version u8 | flags u8 | seq u32 | len u16 | crc32 u32 | fields
//! ```
//!
//! `fields` is `len` bytes of `tag u8, length u8, value`. Unknown tags are
//! skipped, so an older firmware can read a newer record.

use core::fmt;

use crate::http::BaseUrl;

pub const SSID_MAX: usize = 32;
/// WPA2 passphrases are 8 to 63 characters; 64 hex digits is a raw key.
pub const PASSWORD_MAX: usize = 64;
pub const SERVER_URL_MAX: usize = 64;

/// Offset of each record in the settings partition; one record per sector.
pub const SLOT_OFFSETS: [u32; 2] = [0, 4096];
/// Bytes to read for one record: header plus the largest possible fields.
pub const RECORD_MAX: usize = HEADER_LEN + 3 * 2 + SSID_MAX + PASSWORD_MAX + SERVER_URL_MAX;

const MAGIC: u32 = 0x554c_5354; // "ULST"
const VERSION: u8 = 1;
const HEADER_LEN: usize = 16;

const TAG_WIFI_SSID: u8 = 1;
const TAG_WIFI_PASSWORD: u8 = 2;
const TAG_SERVER_URL: u8 = 3;

/// A short text with a fixed capacity, without an allocator.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Text<const N: usize> {
    bytes: [u8; N],
    len: u8,
}

impl<const N: usize> Text<N> {
    pub const fn empty() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    /// `None` when `text` is longer than `N` bytes.
    pub fn new(text: &str) -> Option<Self> {
        let mut out = Self::empty();
        out.push_str(text).then_some(out)
    }

    /// Appends `text`; returns false (and appends nothing) when it does not fit.
    pub fn push_str(&mut self, text: &str) -> bool {
        let len = usize::from(self.len);
        if len + text.len() > N {
            return false;
        }
        self.bytes[len..len + text.len()].copy_from_slice(text.as_bytes());
        self.len += text.len() as u8;
        true
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Self::new(core::str::from_utf8(bytes).ok()?)
    }
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self::empty()
    }
}

/// Fails, writing nothing of the piece, when it does not fit.
impl<const N: usize> fmt::Write for Text<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.push_str(text) {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

impl<const N: usize> fmt::Debug for Text<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    pub wifi_ssid: Text<SSID_MAX>,
    pub wifi_password: Text<PASSWORD_MAX>,
    /// `http://<IPv4>:<port>`, or empty when there is no fleet server.
    pub server_url: Text<SERVER_URL_MAX>,
}

/// Why settings were refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Invalid {
    MissingSsid,
    SsidTooLong,
    /// WPA2 needs 8 to 63 characters, or exactly 64 hex digits.
    BadPassword,
    /// Not `http://<IPv4>[:port]`.
    BadServerUrl,
}

impl Invalid {
    /// A sentence for the setup page.
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingSsid => "Enter the Wi-Fi name.",
            Self::SsidTooLong => "The Wi-Fi name is longer than 32 bytes.",
            Self::BadPassword => {
                "The password must be empty (open network) or 8 to 63 characters long."
            }
            Self::BadServerUrl => {
                "The server address must look like http://192.168.1.10:3000 (no names, IPv4 only)."
            }
        }
    }
}

impl Settings {
    /// Checks and builds settings from user input.
    pub fn new(ssid: &str, password: &str, server_url: &str) -> Result<Self, Invalid> {
        let server_url = server_url.trim();
        if ssid.is_empty() {
            return Err(Invalid::MissingSsid);
        }
        if !valid_password(password) {
            return Err(Invalid::BadPassword);
        }
        if !server_url.is_empty() && BaseUrl::parse(server_url).is_none() {
            return Err(Invalid::BadServerUrl);
        }
        Ok(Self {
            wifi_ssid: Text::new(ssid).ok_or(Invalid::SsidTooLong)?,
            wifi_password: Text::new(password).ok_or(Invalid::BadPassword)?,
            server_url: Text::new(server_url).ok_or(Invalid::BadServerUrl)?,
        })
    }

    /// The board can try to join a network.
    pub fn has_wifi(&self) -> bool {
        !self.wifi_ssid.is_empty()
    }
}

fn valid_password(password: &str) -> bool {
    match password.len() {
        0 | 8..=63 => true,
        64 => password.bytes().all(|byte| byte.is_ascii_hexdigit()),
        _ => false,
    }
}

/// A record read back from one slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stored {
    pub seq: u32,
    pub settings: Settings,
}

/// Writes a record into `out` and returns its length.
pub fn encode(settings: &Settings, seq: u32, out: &mut [u8; RECORD_MAX]) -> usize {
    out.fill(0xff);
    let mut len = HEADER_LEN;
    for (tag, value) in [
        (TAG_WIFI_SSID, settings.wifi_ssid.as_str()),
        (TAG_WIFI_PASSWORD, settings.wifi_password.as_str()),
        (TAG_SERVER_URL, settings.server_url.as_str()),
    ] {
        out[len] = tag;
        out[len + 1] = value.len() as u8;
        out[len + 2..len + 2 + value.len()].copy_from_slice(value.as_bytes());
        len += 2 + value.len();
    }
    let fields_len = (len - HEADER_LEN) as u16;
    out[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    out[4] = VERSION;
    out[5] = 0; // flags, none yet
    out[6..10].copy_from_slice(&seq.to_le_bytes());
    out[10..12].copy_from_slice(&fields_len.to_le_bytes());
    let crc = crc32(&[&out[..12], &out[HEADER_LEN..len]]);
    out[12..16].copy_from_slice(&crc.to_le_bytes());
    len
}

/// Reads a record; `None` for an erased, torn or foreign slot.
pub fn decode(bytes: &[u8]) -> Option<Stored> {
    let header = bytes.get(..HEADER_LEN)?;
    if u32::from_le_bytes(header[0..4].try_into().ok()?) != MAGIC || header[4] != VERSION {
        return None;
    }
    let seq = u32::from_le_bytes(header[6..10].try_into().ok()?);
    let fields_len = usize::from(u16::from_le_bytes(header[10..12].try_into().ok()?));
    let crc = u32::from_le_bytes(header[12..16].try_into().ok()?);
    let fields = bytes.get(HEADER_LEN..HEADER_LEN + fields_len)?;
    if crc32(&[&header[..12], fields]) != crc {
        return None;
    }

    let mut settings = Settings::default();
    let mut rest = fields;
    while let [tag, len, tail @ ..] = rest {
        let value = tail.get(..usize::from(*len))?;
        match *tag {
            TAG_WIFI_SSID => settings.wifi_ssid = Text::from_bytes(value)?,
            TAG_WIFI_PASSWORD => settings.wifi_password = Text::from_bytes(value)?,
            TAG_SERVER_URL => settings.server_url = Text::from_bytes(value)?,
            _ => {}
        }
        rest = &tail[usize::from(*len)..];
    }
    if !rest.is_empty() {
        return None;
    }
    Some(Stored { seq, settings })
}

/// Which slot holds the current record, from what each slot decoded to.
pub fn newest(slots: [Option<Stored>; 2]) -> Option<(usize, Stored)> {
    match slots {
        [Some(a), Some(b)] => Some(if (b.seq.wrapping_sub(a.seq) as i32) > 0 {
            (1, b)
        } else {
            (0, a)
        }),
        [Some(a), None] => Some((0, a)),
        [None, Some(b)] => Some((1, b)),
        [None, None] => None,
    }
}

/// Where the next save goes and its sequence number: the slot that does not
/// hold the current record.
pub fn next_write(current: Option<(usize, Stored)>) -> (usize, u32) {
    match current {
        Some((slot, stored)) => (1 - slot, stored.seq.wrapping_add(1)),
        None => (0, 1),
    }
}

/// CRC-32 (IEEE) over several parts.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for byte in parts.iter().flat_map(|part| part.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Settings {
        Settings::new("home", "correct horse", "http://192.168.1.10:3000").unwrap()
    }

    #[test]
    fn round_trip() {
        let mut buf = [0u8; RECORD_MAX];
        let len = encode(&sample(), 7, &mut buf);
        let stored = decode(&buf[..len]).unwrap();
        assert_eq!(stored.seq, 7);
        assert_eq!(stored.settings, sample());
        // Trailing erased flash after the record is fine.
        assert_eq!(decode(&buf).unwrap().settings, sample());
    }

    #[test]
    fn longest_fields_fit() {
        let settings = Settings::new(
            &"s".repeat(SSID_MAX),
            &"a".repeat(PASSWORD_MAX),
            "http://255.255.255.255:65535",
        )
        .unwrap();
        let mut buf = [0u8; RECORD_MAX];
        encode(&settings, 1, &mut buf);
        assert_eq!(decode(&buf).unwrap().settings, settings);
    }

    #[test]
    fn erased_or_torn_records_are_ignored() {
        assert_eq!(decode(&[0xff; RECORD_MAX]), None);
        let mut buf = [0u8; RECORD_MAX];
        let len = encode(&sample(), 1, &mut buf);
        buf[len - 1] ^= 0x20;
        assert_eq!(decode(&buf), None);
        let len = encode(&sample(), 1, &mut buf);
        assert_eq!(decode(&buf[..len - 1]), None);
    }

    #[test]
    fn newest_wins_across_wrap() {
        let stored = |seq| {
            Some(Stored {
                seq,
                settings: sample(),
            })
        };
        assert_eq!(newest([stored(1), stored(2)]).unwrap().0, 1);
        assert_eq!(newest([stored(3), stored(2)]).unwrap().0, 0);
        assert_eq!(newest([stored(u32::MAX), stored(0)]).unwrap().0, 1);
        assert_eq!(newest([None, stored(5)]).unwrap().0, 1);
        assert_eq!(newest([None, None]), None);
    }

    #[test]
    fn saves_alternate_slots() {
        assert_eq!(next_write(None), (0, 1));
        let current = Stored {
            seq: 4,
            settings: sample(),
        };
        assert_eq!(next_write(Some((0, current))), (1, 5));
        assert_eq!(next_write(Some((1, current))), (0, 5));
    }

    #[test]
    fn validates_input() {
        assert_eq!(Settings::new("", "", "").unwrap_err(), Invalid::MissingSsid);
        assert_eq!(
            Settings::new("x", "short", "").unwrap_err(),
            Invalid::BadPassword
        );
        assert!(Settings::new("x", "", "").is_ok());
        assert!(Settings::new("x", &"a1".repeat(32), "").is_ok());
        assert_eq!(
            Settings::new("x", &"zz".repeat(32), "").unwrap_err(),
            Invalid::BadPassword
        );
        assert_eq!(
            Settings::new("x", "", "http://fleet.local:3000").unwrap_err(),
            Invalid::BadServerUrl
        );
        assert_eq!(
            Settings::new(&"s".repeat(33), "", "").unwrap_err(),
            Invalid::SsidTooLong
        );
        assert_eq!(
            Settings::new("x", "", " http://10.0.0.2:3000 ")
                .unwrap()
                .server_url
                .as_str(),
            "http://10.0.0.2:3000"
        );
    }

    #[test]
    fn crc_matches_ieee() {
        assert_eq!(crc32(&[b"123456789"]), 0xcbf4_3926);
    }
}
