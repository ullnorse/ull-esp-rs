//! The ESP-IDF `otadata` format and the rules for choosing boot and update slots.
//!
//! `otadata` holds two 32-byte select entries, one per flash sector. Each entry
//! carries a sequence number, an image state and a CRC of the sequence. The
//! bootloader boots OTA partition `(seq - 1) % ota_partition_count` from the
//! valid entry with the highest sequence. Writes alternate between the two
//! sectors, so a power cut during a write leaves the other entry intact.
//!
//! OTA partitions are named by number here: `0` is `ota_0`, `1` is `ota_1`, and
//! so on. The factory partition is not an OTA partition and has no number.

pub const SLOT_LEN: usize = 32;
pub const SECOND_SLOT_OFFSET: u32 = 0x1000;
pub const UNINITIALIZED_SEQUENCE: u32 = u32::MAX;
/// ESP-IDF supports `ota_0` through `ota_15`.
pub const MAX_OTA_PARTITIONS: usize = 16;

/// Raw image states, as stored in a select entry.
pub mod state {
    pub const NEW: u32 = 0x0;
    pub const PENDING_VERIFY: u32 = 0x1;
    pub const VALID: u32 = 0x2;
    pub const INVALID: u32 = 0x3;
    pub const ABORTED: u32 = 0x4;
    pub const UNDEFINED: u32 = 0xFFFF_FFFF;
}

/// One otadata select entry. Bytes 4..24 (the label) are unused and stay `0xff`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SelectEntry {
    pub sequence: u32,
    pub state: u32,
    pub crc: u32,
}

impl SelectEntry {
    /// An entry with a correct CRC for `sequence`.
    pub const fn new(sequence: u32, state: u32) -> Self {
        Self {
            sequence,
            state,
            crc: sequence_crc(sequence),
        }
    }

    pub fn decode(bytes: &[u8; SLOT_LEN]) -> Self {
        let word =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        Self {
            sequence: word(0),
            state: word(24),
            crc: word(28),
        }
    }

    pub fn encode(&self) -> [u8; SLOT_LEN] {
        let mut bytes = [0xff; SLOT_LEN];
        bytes[0..4].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.state.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.crc.to_le_bytes());
        bytes
    }

    /// Whether the bootloader may select this entry: initialized, CRC correct,
    /// and not marked invalid or aborted.
    pub fn is_valid(&self) -> bool {
        self.sequence != UNINITIALIZED_SEQUENCE
            && self.state != state::INVALID
            && self.state != state::ABORTED
            && self.crc == sequence_crc(self.sequence)
    }
}

/// Where a select entry has to be written.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EntryWrite {
    /// 0 or 1.
    pub slot: usize,
    pub entry: SelectEntry,
}

impl EntryWrite {
    /// Byte offset of the slot inside the otadata partition.
    pub fn offset(&self) -> u32 {
        slot_offset(self.slot)
    }
}

pub const fn slot_offset(slot: usize) -> u32 {
    if slot == 0 { 0 } else { SECOND_SLOT_OFFSET }
}

/// CRC-32 (IEEE, reflected) of the little-endian sequence, as computed by the
/// ESP ROM's `crc32_le(UINT32_MAX, ...)`.
pub const fn sequence_crc(sequence: u32) -> u32 {
    let bytes = sequence.to_le_bytes();
    let mut crc = 0u32;
    let mut index = 0;
    while index < bytes.len() {
        crc ^= bytes[index] as u32;
        let mut bit = 0;
        while bit < 8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            bit += 1;
        }
        index += 1;
    }
    !crc
}

/// The entry the bootloader uses: the valid one with the higher sequence.
pub fn active_slot(entries: &[SelectEntry; 2]) -> Option<usize> {
    match (entries[0].is_valid(), entries[1].is_valid()) {
        (true, true) => Some(if entries[0].sequence >= entries[1].sequence {
            0
        } else {
            1
        }),
        (true, false) => Some(0),
        (false, true) => Some(1),
        (false, false) => None,
    }
}

/// OTA partition number a sequence selects.
pub fn partition_for_sequence(sequence: u32, ota_partition_count: usize) -> usize {
    debug_assert!(ota_partition_count > 0);
    (sequence.wrapping_sub(1) % ota_partition_count as u32) as usize
}

/// The OTA partition the bootloader will pick, with its raw state.
pub fn selected_partition(
    entries: &[SelectEntry; 2],
    ota_partition_count: usize,
) -> Option<(usize, u32)> {
    if ota_partition_count == 0 {
        return None;
    }
    let active = entries[active_slot(entries)?];
    Some((
        partition_for_sequence(active.sequence, ota_partition_count),
        active.state,
    ))
}

/// The write that makes the bootloader select `partition` next, in `state`.
///
/// The new entry goes into the inactive slot with the smallest sequence that
/// maps to `partition` and is strictly greater than the active one, so it
/// always wins, even when the same partition is selected again.
pub fn select_partition(
    entries: &[SelectEntry; 2],
    partition: usize,
    ota_partition_count: usize,
    state: u32,
) -> EntryWrite {
    debug_assert!(partition < ota_partition_count);
    let count = ota_partition_count as u32;
    let mut sequence = partition as u32 + 1;

    let slot = match active_slot(entries) {
        Some(active) => {
            let current = entries[active].sequence;
            if current >= sequence {
                sequence += (current - sequence) / count * count + count;
            }
            active ^ 1
        }
        None => 0,
    };

    EntryWrite {
        slot,
        entry: SelectEntry::new(sequence, state),
    }
}

/// The write that changes the state of the active entry in place, or `None`
/// when no entry is active.
pub fn set_active_state(entries: &[SelectEntry; 2], state: u32) -> Option<EntryWrite> {
    let slot = active_slot(entries)?;
    Some(EntryWrite {
        slot,
        entry: SelectEntry {
            state,
            ..entries[slot]
        },
    })
}

/// The OTA partition to download the next update into.
///
/// `selected` is the partition otadata selects (`None` for factory or blank
/// otadata) and `booted` is the OTA partition running now (`None` for
/// factory). The target is the one after `selected`, skipping the running
/// image so it is never overwritten.
pub fn next_update_partition(
    selected: Option<usize>,
    booted: Option<usize>,
    ota_partition_count: usize,
) -> usize {
    debug_assert!(ota_partition_count >= 2);
    let after = |partition: Option<usize>| partition.map_or(0, |p| (p + 1) % ota_partition_count);

    let next = after(selected);
    if booted == Some(next) {
        after(Some(next))
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLANK: SelectEntry = SelectEntry {
        sequence: UNINITIALIZED_SEQUENCE,
        state: state::UNDEFINED,
        crc: UNINITIALIZED_SEQUENCE,
    };

    #[test]
    fn crc_matches_rom_crc32_le() {
        // zlib.crc32(struct.pack('<I', seq), 0xFFFFFFFF)
        assert_eq!(sequence_crc(1), 0x4743_989a);
        assert_eq!(sequence_crc(2), 0x55f6_3774);
        assert_eq!(sequence_crc(20), 0x2084_3f37);
    }

    #[test]
    fn encode_decode_round_trip_keeps_label_erased() {
        let entry = SelectEntry::new(20, state::NEW);
        let bytes = entry.encode();
        assert!(bytes[4..24].iter().all(|byte| *byte == 0xff));
        assert_eq!(SelectEntry::decode(&bytes), entry);
    }

    #[test]
    fn validity_rules() {
        assert!(SelectEntry::new(3, state::VALID).is_valid());
        assert!(SelectEntry::new(3, state::UNDEFINED).is_valid());
        assert!(!BLANK.is_valid());
        assert!(!SelectEntry::new(3, state::INVALID).is_valid());
        assert!(!SelectEntry::new(3, state::ABORTED).is_valid());
        assert!(
            !SelectEntry {
                crc: 0,
                ..SelectEntry::new(3, state::VALID)
            }
            .is_valid()
        );
    }

    #[test]
    fn active_slot_prefers_higher_valid_sequence() {
        let a = SelectEntry::new(4, state::VALID);
        let b = SelectEntry::new(5, state::NEW);
        assert_eq!(active_slot(&[BLANK, BLANK]), None);
        assert_eq!(active_slot(&[a, BLANK]), Some(0));
        assert_eq!(active_slot(&[BLANK, b]), Some(1));
        assert_eq!(active_slot(&[a, b]), Some(1));
        assert_eq!(active_slot(&[b, a]), Some(0));
        assert_eq!(active_slot(&[a, a]), Some(0));
        let rejected = SelectEntry::new(9, state::INVALID);
        assert_eq!(active_slot(&[a, rejected]), Some(0));
    }

    #[test]
    fn sequence_maps_to_partition() {
        assert_eq!(partition_for_sequence(1, 2), 0);
        assert_eq!(partition_for_sequence(2, 2), 1);
        assert_eq!(partition_for_sequence(20, 2), 1);
        assert_eq!(partition_for_sequence(7, 3), 0);
    }

    #[test]
    fn selected_partition_reads_active_entry() {
        assert_eq!(selected_partition(&[BLANK, BLANK], 2), None);
        let entries = [
            SelectEntry::new(19, state::VALID),
            SelectEntry::new(20, state::NEW),
        ];
        assert_eq!(selected_partition(&entries, 2), Some((1, state::NEW)));
        assert_eq!(selected_partition(&entries, 0), None);
    }

    #[test]
    fn first_selection_uses_slot_zero() {
        let write = select_partition(&[BLANK, BLANK], 1, 2, state::VALID);
        assert_eq!(write.slot, 0);
        assert_eq!(write.offset(), 0);
        assert_eq!(write.entry, SelectEntry::new(2, state::VALID));
    }

    #[test]
    fn selection_alternates_slots_and_increases_sequence() {
        let entries = [SelectEntry::new(19, state::VALID), BLANK];
        let write = select_partition(&entries, 1, 2, state::NEW);
        assert_eq!(write.slot, 1);
        assert_eq!(write.offset(), SECOND_SLOT_OFFSET);
        assert_eq!(write.entry.sequence, 20);
        assert_eq!(partition_for_sequence(write.entry.sequence, 2), 1);
    }

    #[test]
    fn reselecting_same_partition_still_wins() {
        let entries = [SelectEntry::new(20, state::VALID), BLANK];
        let write = select_partition(&entries, 1, 2, state::NEW);
        assert_eq!(write.entry.sequence, 22);

        let mut after = entries;
        after[write.slot] = write.entry;
        assert_eq!(active_slot(&after), Some(write.slot));
        assert_eq!(selected_partition(&after, 2), Some((1, state::NEW)));
    }

    #[test]
    fn selection_always_wins_for_every_layout() {
        for count in 2..=MAX_OTA_PARTITIONS {
            for current in 1..40u32 {
                for partition in 0..count {
                    let entries = [BLANK, SelectEntry::new(current, state::VALID)];
                    let write = select_partition(&entries, partition, count, state::NEW);
                    let mut after = entries;
                    after[write.slot] = write.entry;
                    assert_eq!(write.slot, 0);
                    assert!(write.entry.sequence > current);
                    assert!(write.entry.sequence <= current + count as u32);
                    assert_eq!(
                        selected_partition(&after, count),
                        Some((partition, state::NEW))
                    );
                }
            }
        }
    }

    #[test]
    fn set_active_state_rewrites_active_slot() {
        assert_eq!(set_active_state(&[BLANK, BLANK], state::VALID), None);
        let entries = [
            SelectEntry::new(19, state::VALID),
            SelectEntry::new(20, state::PENDING_VERIFY),
        ];
        let write = set_active_state(&entries, state::VALID).unwrap();
        assert_eq!(write.slot, 1);
        assert_eq!(write.entry, SelectEntry::new(20, state::VALID));
    }

    #[test]
    fn rejecting_active_falls_back_to_other_slot() {
        let entries = [
            SelectEntry::new(19, state::VALID),
            SelectEntry::new(20, state::PENDING_VERIFY),
        ];
        let write = set_active_state(&entries, state::INVALID).unwrap();
        let mut after = entries;
        after[write.slot] = write.entry;
        assert_eq!(selected_partition(&after, 2), Some((0, state::VALID)));
    }

    #[test]
    fn next_update_partition_skips_running_image() {
        // Factory or blank otadata: start at ota_0.
        assert_eq!(next_update_partition(None, None, 2), 0);
        // Factoryless, running ota_0 with blank otadata.
        assert_eq!(next_update_partition(None, Some(0), 2), 1);
        // Normal case: running what is selected.
        assert_eq!(next_update_partition(Some(0), Some(0), 2), 1);
        assert_eq!(next_update_partition(Some(1), Some(1), 2), 0);
        assert_eq!(next_update_partition(Some(2), Some(2), 3), 0);
        // Selected image not booted yet (e.g. after a rollback).
        assert_eq!(next_update_partition(Some(0), Some(1), 3), 2);
    }

    #[test]
    fn next_update_partition_never_targets_running_image() {
        for count in 2..=MAX_OTA_PARTITIONS {
            for booted in 0..count {
                for selected in (0..count).map(Some).chain([None]) {
                    let target = next_update_partition(selected, Some(booted), count);
                    assert!(target < count);
                    assert_ne!(target, booted, "count={count} selected={selected:?}");
                }
            }
        }
    }
}
