use embedded_storage::{ReadStorage, Storage};
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::{
    AppPartitionSubType, DataPartitionSubType, FlashRegion, PartitionEntry, PartitionTable,
    PartitionType,
};
use esp_storage::FlashStorage;

use super::{
    OTA_DATA_SECOND_SLOT_OFFSET, OTA_DATA_SLOT_LEN, OTA_DATA_UNINITIALIZED_SEQUENCE, OtaError,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) struct RawOtaSelectEntry {
    pub ota_seq: u32,
    pub ota_state_raw: u32,
    pub crc: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) struct RawOtaDataStatus {
    pub selected_partition: Option<AppPartitionSubType>,
    pub ota_state: Option<OtaImageState>,
}

pub(super) fn align_up(value: u32, alignment: u32) -> u32 {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

pub(super) fn app_partition_subtype(
    entry: PartitionEntry<'_>,
) -> Result<AppPartitionSubType, OtaError> {
    match entry.partition_type() {
        PartitionType::App(subtype) => Ok(subtype),
        _ => Err(OtaError::InvalidBootedPartition),
    }
}

pub(super) fn is_ota_partition(partition: AppPartitionSubType) -> bool {
    partition != AppPartitionSubType::Factory && partition != AppPartitionSubType::Test
}

pub(super) fn raw_otadata_status(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
    ota_partition_count: usize,
) -> Result<RawOtaDataStatus, OtaError> {
    if ota_partition_count == 0 {
        return Ok(RawOtaDataStatus {
            selected_partition: None,
            ota_state: None,
        });
    }

    let (slot0, slot1) = read_otadata_entries(flash, pt)?;
    let active_slot = active_otadata_slot(slot0, slot1);

    let Some(active_slot) = active_slot else {
        return Ok(RawOtaDataStatus {
            selected_partition: None,
            ota_state: None,
        });
    };

    let active = if active_slot == 0 { slot0 } else { slot1 };
    let partition_number = active
        .ota_seq
        .wrapping_sub(1)
        .wrapping_rem(ota_partition_count as u32) as u8;

    Ok(RawOtaDataStatus {
        selected_partition: Some(ota_partition_from_number(partition_number)?),
        ota_state: OtaImageState::try_from(active.ota_state_raw).ok(),
    })
}

pub(super) fn write_selected_otadata(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
    ota_partition_count: usize,
    partition: AppPartitionSubType,
    state: OtaImageState,
) -> Result<(), OtaError> {
    if !is_ota_partition(partition) {
        return Err(OtaError::NoBootablePartition);
    }

    let partition_number = ota_partition_number(partition)? as u32;
    let (slot0, slot1) = read_otadata_entries(flash, pt)?;
    let active_slot = active_otadata_slot(slot0, slot1);
    let next_slot = active_slot.map_or(0, |slot| slot ^ 1);
    let new_seq = match active_slot {
        Some(slot) => next_sequence_for_partition(
            if slot == 0 { slot0 } else { slot1 }.ota_seq,
            partition_number,
            ota_partition_count,
        ),
        None => partition_number + 1,
    };

    write_otadata_entry(
        flash,
        pt,
        next_slot,
        RawOtaSelectEntry {
            ota_seq: new_seq,
            ota_state_raw: state as u32,
            crc: ota_sequence_crc(new_seq),
        },
    )
}

pub(super) fn update_active_otadata_state(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
    ota_partition_count: usize,
    state: OtaImageState,
) -> Result<(), OtaError> {
    if ota_partition_count == 0 {
        return Err(esp_bootloader_esp_idf::partitions::Error::InvalidState.into());
    }

    let (slot0, slot1) = read_otadata_entries(flash, pt)?;
    let active_slot = active_otadata_slot(slot0, slot1)
        .ok_or(esp_bootloader_esp_idf::partitions::Error::InvalidState)?;
    let mut entry = if active_slot == 0 { slot0 } else { slot1 };
    entry.ota_state_raw = state as u32;

    write_otadata_entry(flash, pt, active_slot, entry)
}

pub(super) fn read_otadata_entries(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
) -> Result<(RawOtaSelectEntry, RawOtaSelectEntry), OtaError> {
    let ota_partition = pt
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))?
        .ok_or(esp_bootloader_esp_idf::partitions::Error::Invalid)?;
    let mut ota_partition = ota_partition.as_embedded_storage(flash);

    Ok((
        read_ota_select_entry(&mut ota_partition, 0)?,
        read_ota_select_entry(&mut ota_partition, OTA_DATA_SECOND_SLOT_OFFSET)?,
    ))
}

pub(super) fn active_otadata_slot(
    slot0: RawOtaSelectEntry,
    slot1: RawOtaSelectEntry,
) -> Option<usize> {
    let slot0_valid = is_valid_otadata_slot(slot0);
    let slot1_valid = is_valid_otadata_slot(slot1);

    match (slot0_valid, slot1_valid) {
        (true, true) => Some(if slot0.ota_seq >= slot1.ota_seq { 0 } else { 1 }),
        (true, false) => Some(0),
        (false, true) => Some(1),
        (false, false) => None,
    }
}

pub(super) fn is_valid_otadata_slot(slot: RawOtaSelectEntry) -> bool {
    slot.ota_seq != OTA_DATA_UNINITIALIZED_SEQUENCE
        && slot.ota_state_raw != OtaImageState::Invalid as u32
        && slot.ota_state_raw != OtaImageState::Aborted as u32
        && slot.crc == ota_sequence_crc(slot.ota_seq)
}

pub(super) fn ota_sequence_crc(sequence: u32) -> u32 {
    let mut crc = 0u32;

    for byte in sequence.to_le_bytes() {
        crc ^= byte as u32;

        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }

    crc ^ u32::MAX
}

pub(super) fn count_ota_partitions(pt: &PartitionTable<'_>) -> usize {
    pt.iter()
        .filter(|entry| {
            matches!(
                entry.partition_type(),
                PartitionType::App(subtype)
                    if subtype != AppPartitionSubType::Factory && subtype != AppPartitionSubType::Test
            )
        })
        .count()
}

pub(super) fn next_ota_partition(
    selected_partition: AppPartitionSubType,
    booted_partition: Option<AppPartitionSubType>,
    ota_partition_count: usize,
) -> Result<AppPartitionSubType, OtaError> {
    let next_partition = match selected_partition {
        AppPartitionSubType::Factory => AppPartitionSubType::Ota0,
        _ => ota_partition_from_number(
            (ota_partition_number(selected_partition)? + 1) % ota_partition_count as u8,
        )?,
    };

    if booted_partition == Some(next_partition) {
        return match selected_partition {
            AppPartitionSubType::Factory => {
                ota_partition_from_number(1 % ota_partition_count as u8)
            }
            _ => ota_partition_from_number(
                (ota_partition_number(selected_partition)? + 2) % ota_partition_count as u8,
            ),
        };
    }

    Ok(next_partition)
}

fn read_ota_select_entry(
    ota_partition: &mut FlashRegion<'_, FlashStorage<'_>>,
    offset: u32,
) -> Result<RawOtaSelectEntry, OtaError> {
    let mut bytes = [0u8; OTA_DATA_SLOT_LEN];
    ota_partition.read(offset, &mut bytes)?;

    Ok(RawOtaSelectEntry {
        ota_seq: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        ota_state_raw: u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        crc: u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
    })
}

fn write_otadata_entry(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
    slot: usize,
    entry: RawOtaSelectEntry,
) -> Result<(), OtaError> {
    let ota_partition = pt
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))?
        .ok_or(esp_bootloader_esp_idf::partitions::Error::Invalid)?;
    let mut ota_partition = ota_partition.as_embedded_storage(flash);
    let bytes = encode_ota_select_entry(entry);
    ota_partition.write(ota_slot_offset(slot), &bytes)?;
    Ok(())
}

fn encode_ota_select_entry(entry: RawOtaSelectEntry) -> [u8; OTA_DATA_SLOT_LEN] {
    let mut bytes = [0xff; OTA_DATA_SLOT_LEN];
    bytes[0..4].copy_from_slice(&entry.ota_seq.to_le_bytes());
    bytes[24..28].copy_from_slice(&entry.ota_state_raw.to_le_bytes());
    bytes[28..32].copy_from_slice(&entry.crc.to_le_bytes());
    bytes
}

fn ota_slot_offset(slot: usize) -> u32 {
    if slot == 0 {
        0
    } else {
        OTA_DATA_SECOND_SLOT_OFFSET
    }
}

fn next_sequence_for_partition(
    current_sequence: u32,
    partition_number: u32,
    ota_partition_count: usize,
) -> u32 {
    let mut sequence = partition_number + 1;

    while current_sequence > sequence {
        sequence += ota_partition_count as u32;
    }

    sequence
}

fn ota_partition_number(partition: AppPartitionSubType) -> Result<u8, OtaError> {
    match partition {
        AppPartitionSubType::Ota0 => Ok(0),
        AppPartitionSubType::Ota1 => Ok(1),
        AppPartitionSubType::Ota2 => Ok(2),
        AppPartitionSubType::Ota3 => Ok(3),
        AppPartitionSubType::Ota4 => Ok(4),
        AppPartitionSubType::Ota5 => Ok(5),
        AppPartitionSubType::Ota6 => Ok(6),
        AppPartitionSubType::Ota7 => Ok(7),
        AppPartitionSubType::Ota8 => Ok(8),
        AppPartitionSubType::Ota9 => Ok(9),
        AppPartitionSubType::Ota10 => Ok(10),
        AppPartitionSubType::Ota11 => Ok(11),
        AppPartitionSubType::Ota12 => Ok(12),
        AppPartitionSubType::Ota13 => Ok(13),
        AppPartitionSubType::Ota14 => Ok(14),
        AppPartitionSubType::Ota15 => Ok(15),
        _ => Err(OtaError::NoBootablePartition),
    }
}

fn ota_partition_from_number(number: u8) -> Result<AppPartitionSubType, OtaError> {
    match number {
        0 => Ok(AppPartitionSubType::Ota0),
        1 => Ok(AppPartitionSubType::Ota1),
        2 => Ok(AppPartitionSubType::Ota2),
        3 => Ok(AppPartitionSubType::Ota3),
        4 => Ok(AppPartitionSubType::Ota4),
        5 => Ok(AppPartitionSubType::Ota5),
        6 => Ok(AppPartitionSubType::Ota6),
        7 => Ok(AppPartitionSubType::Ota7),
        8 => Ok(AppPartitionSubType::Ota8),
        9 => Ok(AppPartitionSubType::Ota9),
        10 => Ok(AppPartitionSubType::Ota10),
        11 => Ok(AppPartitionSubType::Ota11),
        12 => Ok(AppPartitionSubType::Ota12),
        13 => Ok(AppPartitionSubType::Ota13),
        14 => Ok(AppPartitionSubType::Ota14),
        15 => Ok(AppPartitionSubType::Ota15),
        _ => Err(OtaError::NoBootablePartition),
    }
}
