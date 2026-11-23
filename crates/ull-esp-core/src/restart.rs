//! Why the device last restarted.
//!
//! The chip's reset reason cannot tell a panic from a planned reboot: both are
//! software resets. So before resetting on purpose, or from the panic handler,
//! the firmware leaves a [`Note`] in RAM that survives the reset. The next boot
//! combines the chip's reason with the note into a [`Restart`].
//!
//! The note is a fixed array of words with a magic value and a checksum, so
//! memory that was never written, or was written by an image with another
//! layout, is ignored.

use core::fmt;

/// Longest detail text kept across the reset, in bytes.
pub const DETAIL_LEN: usize = 120;
/// Size of an encoded note: magic, kind and length, checksum, then the detail.
pub const NOTE_WORDS: usize = 3 + DETAIL_LEN / 4;

const MAGIC: u32 = 0x554c_5253; // "ULRS"

/// The chip's own reset reason, reduced to what matters for a report.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChipReset {
    PowerOn,
    /// Software reset: a planned reboot or a panic, told apart by the note.
    Software,
    /// A hardware watchdog fired: the firmware stopped running tasks.
    Watchdog,
    Brownout,
    DeepSleep,
    Other,
}

/// Why the firmware reset itself on purpose. Stored in the note.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NoteKind {
    /// The panic handler reset the device.
    Panic = 1,
    /// A task stopped reporting progress (software watchdog).
    Stall = 2,
    /// An update was installed.
    Update = 3,
    /// A freshly updated image gave up and returned to the previous one.
    Rollback = 4,
    /// A reboot someone asked for.
    Requested = 5,
}

impl NoteKind {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Panic,
            2 => Self::Stall,
            3 => Self::Update,
            4 => Self::Rollback,
            5 => Self::Requested,
            _ => return None,
        })
    }
}

/// A note read back after the reset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Note {
    pub kind: NoteKind,
    detail: [u8; DETAIL_LEN],
    detail_len: u8,
}

impl Note {
    pub fn detail(&self) -> &str {
        core::str::from_utf8(&self.detail[..usize::from(self.detail_len)]).unwrap_or("")
    }
}

/// Writes a note into `words`. `detail` is cut at a char boundary to fit.
pub fn encode(kind: NoteKind, detail: &str, words: &mut [u32; NOTE_WORDS]) {
    let detail = truncate(detail, DETAIL_LEN);
    let mut bytes = [0u8; DETAIL_LEN];
    bytes[..detail.len()].copy_from_slice(detail.as_bytes());
    words[0] = MAGIC;
    words[1] = u32::from(kind as u8) | (detail.len() as u32) << 8;
    for (word, chunk) in words[3..].iter_mut().zip(bytes.chunks_exact(4)) {
        *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    words[2] = checksum(words);
}

/// Reads a note back; `None` when `words` holds no valid note.
pub fn decode(words: &[u32; NOTE_WORDS]) -> Option<Note> {
    if words[0] != MAGIC || words[2] != checksum(words) {
        return None;
    }
    let kind = NoteKind::from_u8(words[1] as u8)?;
    let detail_len = (words[1] >> 8) as usize;
    if detail_len > DETAIL_LEN {
        return None;
    }
    let mut detail = [0u8; DETAIL_LEN];
    for (chunk, word) in detail.chunks_exact_mut(4).zip(&words[3..]) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    core::str::from_utf8(&detail[..detail_len]).ok()?;
    Some(Note {
        kind,
        detail,
        detail_len: detail_len as u8,
    })
}

/// FNV-1a over every word except the checksum itself.
fn checksum(words: &[u32; NOTE_WORDS]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for (index, word) in words.iter().enumerate() {
        if index == 2 {
            continue;
        }
        for byte in word.to_le_bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    hash
}

fn truncate(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Why this boot happened: the chip's reason, refined by the note.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Cause {
    PowerOn,
    Brownout,
    /// The hardware watchdog fired.
    Watchdog,
    DeepSleep,
    /// A software reset with no note: the cause is not known.
    Software,
    Other,
    Panic,
    Stall,
    Update,
    Rollback,
    Requested,
}

impl Cause {
    /// The name sent to the fleet server.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PowerOn => "power_on",
            Self::Brownout => "brownout",
            Self::Watchdog => "watchdog",
            Self::DeepSleep => "deep_sleep",
            Self::Software => "software",
            Self::Other => "other",
            Self::Panic => "panic",
            Self::Stall => "stall",
            Self::Update => "update",
            Self::Rollback => "rollback",
            Self::Requested => "requested",
        }
    }

    /// The firmware failed: a panic, a stall, or a hardware watchdog.
    pub fn is_crash(self) -> bool {
        matches!(self, Self::Panic | Self::Stall | Self::Watchdog)
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The last restart, with the detail the note carried (a panic message, the
/// name of a stalled task, a rollback reason).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Restart {
    pub cause: Cause,
    note: Option<Note>,
}

impl Restart {
    /// Combines the chip's reason with the note left before the reset. A note
    /// counts only after a reset that keeps RAM: on power-on or brownout it is
    /// leftover noise.
    pub fn new(chip: ChipReset, note: Option<Note>) -> Self {
        let note = note.filter(|_| {
            matches!(
                chip,
                ChipReset::Software | ChipReset::Watchdog | ChipReset::Other
            )
        });
        let cause = match (&note, chip) {
            (Some(note), _) => match note.kind {
                NoteKind::Panic => Cause::Panic,
                NoteKind::Stall => Cause::Stall,
                NoteKind::Update => Cause::Update,
                NoteKind::Rollback => Cause::Rollback,
                NoteKind::Requested => Cause::Requested,
            },
            (None, ChipReset::PowerOn) => Cause::PowerOn,
            (None, ChipReset::Software) => Cause::Software,
            (None, ChipReset::Watchdog) => Cause::Watchdog,
            (None, ChipReset::Brownout) => Cause::Brownout,
            (None, ChipReset::DeepSleep) => Cause::DeepSleep,
            (None, ChipReset::Other) => Cause::Other,
        };
        Self { cause, note }
    }

    pub fn detail(&self) -> Option<&str> {
        self.note
            .as_ref()
            .map(Note::detail)
            .filter(|detail| !detail.is_empty())
    }
}

/// A `fmt::Write` into a fixed byte buffer that drops what does not fit, for
/// formatting a panic message without an allocator.
pub struct DetailWriter {
    bytes: [u8; DETAIL_LEN],
    len: usize,
}

impl DetailWriter {
    pub const fn new() -> Self {
        Self {
            bytes: [0; DETAIL_LEN],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Default for DetailWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Write for DetailWriter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let room = DETAIL_LEN - self.len;
        let part = truncate(text, room);
        self.bytes[self.len..self.len + part.len()].copy_from_slice(part.as_bytes());
        self.len += part.len();
        Ok(())
    }
}

/// Shortens a panic location's file path to the part that names the crate:
/// `/home/u/.cargo/registry/src/x/esp-hal-1.2.2/src/i2c.rs` becomes
/// `esp-hal-1.2.2/src/i2c.rs`. Paths without a `src/` part are kept.
pub fn short_path(path: &str) -> &str {
    match path.rfind("/src/") {
        Some(src) => match path[..src].rfind('/') {
            Some(slash) => &path[slash + 1..],
            None => path,
        },
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use core::fmt::Write as _;

    use super::*;

    #[test]
    fn note_round_trip() {
        let mut words = [0u32; NOTE_WORDS];
        encode(NoteKind::Panic, "src/main.rs:12: boom", &mut words);
        let note = decode(&words).unwrap();
        assert_eq!(note.kind, NoteKind::Panic);
        assert_eq!(note.detail(), "src/main.rs:12: boom");
    }

    #[test]
    fn blank_or_corrupt_memory_has_no_note() {
        assert_eq!(decode(&[0; NOTE_WORDS]), None);
        let mut words = [0u32; NOTE_WORDS];
        encode(NoteKind::Stall, "fleet", &mut words);
        words[5] ^= 1;
        assert_eq!(decode(&words), None);
    }

    #[test]
    fn long_detail_is_cut_at_a_char_boundary() {
        let mut words = [0u32; NOTE_WORDS];
        let text = "é".repeat(100); // 200 bytes
        encode(NoteKind::Rollback, &text, &mut words);
        let note = decode(&words).unwrap();
        assert_eq!(note.detail().len(), DETAIL_LEN);
        assert!(note.detail().chars().all(|c| c == 'é'));
    }

    #[test]
    fn note_refines_a_software_reset() {
        let mut words = [0u32; NOTE_WORDS];
        encode(NoteKind::Update, "", &mut words);
        let restart = Restart::new(ChipReset::Software, decode(&words));
        assert_eq!(restart.cause, Cause::Update);
        assert_eq!(restart.detail(), None);

        assert_eq!(
            Restart::new(ChipReset::Software, None).cause,
            Cause::Software
        );
        assert_eq!(
            Restart::new(ChipReset::Watchdog, None).cause,
            Cause::Watchdog
        );
    }

    #[test]
    fn note_is_ignored_after_power_loss() {
        let mut words = [0u32; NOTE_WORDS];
        encode(NoteKind::Panic, "old", &mut words);
        let restart = Restart::new(ChipReset::PowerOn, decode(&words));
        assert_eq!(restart.cause, Cause::PowerOn);
        assert_eq!(restart.detail(), None);
    }

    #[test]
    fn short_paths() {
        assert_eq!(
            short_path("/home/u/.cargo/registry/src/x/esp-hal-1.2.2/src/i2c.rs"),
            "esp-hal-1.2.2/src/i2c.rs"
        );
        assert_eq!(
            short_path("examples/sht3x-ssd1306/src/tasks/sensor.rs"),
            "sht3x-ssd1306/src/tasks/sensor.rs"
        );
        assert_eq!(short_path("src/main.rs"), "src/main.rs");
        assert_eq!(short_path("main.rs"), "main.rs");
    }

    #[test]
    fn detail_writer_drops_overflow() {
        let mut out = DetailWriter::new();
        write!(out, "{}", "x".repeat(DETAIL_LEN + 10)).unwrap();
        assert_eq!(out.as_str().len(), DETAIL_LEN);
        assert!(Cause::Panic.is_crash() && !Cause::Update.is_crash());
    }
}
