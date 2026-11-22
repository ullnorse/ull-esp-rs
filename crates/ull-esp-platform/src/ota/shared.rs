//! Flash access for otadata. The selection rules live in `ull_esp_core::otadata`
//! and are tested on the host; this module only reads and writes the entries.

use esp_bootloader_esp_idf::partitions::{
    AppPartitionSubType, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionEntry,
    PartitionTable, PartitionType, read_partition_table,
};
use esp_storage::FlashStorage;
use ull_esp_core::otadata::{self, EntryWrite, SLOT_LEN, SelectEntry};

use super::OtaError;

/// What the partition table and otadata say about the OTA layout.
pub(super) struct OtaLayout {
    pub ota_partition_count: usize,
    pub booted: Option<AppPartitionSubType>,
    pub factory_present: bool,
}

pub(super) fn with_partition_table<T>(
    flash: &mut FlashStorage<'_>,
    f: impl FnOnce(&mut FlashStorage<'_>, &PartitionTable<'_>) -> Result<T, OtaError>,
) -> Result<T, OtaError> {
    let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    // The table borrows `pt_buffer`, not `flash`, so `flash` can be reused.
    let pt = read_partition_table(flash, &mut pt_buffer)?;
    f(flash, &pt)
}

pub(super) fn layout(pt: &PartitionTable<'_>) -> Result<OtaLayout, OtaError> {
    let ota_partition_count = pt
        .iter()
        .filter(|entry| {
            matches!(entry.partition_type(), PartitionType::App(subtype) if is_ota_partition(subtype))
        })
        .count();

    Ok(OtaLayout {
        ota_partition_count,
        booted: pt
            .booted_partition()?
            .map(app_partition_subtype)
            .transpose()?,
        factory_present: pt
            .find_partition(PartitionType::App(AppPartitionSubType::Factory))?
            .is_some(),
    })
}

pub(super) fn app_partition_subtype(
    entry: PartitionEntry,
) -> Result<AppPartitionSubType, OtaError> {
    match entry.partition_type() {
        PartitionType::App(subtype) => Ok(subtype),
        _ => Err(OtaError::InvalidBootedPartition),
    }
}

pub(super) fn is_ota_partition(partition: AppPartitionSubType) -> bool {
    ota_number(partition).is_some()
}

/// `ota_N` -> `N`; factory and test partitions have no number.
pub(super) fn ota_number(partition: AppPartitionSubType) -> Option<usize> {
    let raw = partition as u8;
    let ota0 = AppPartitionSubType::Ota0 as u8;
    (ota0..ota0 + otadata::MAX_OTA_PARTITIONS as u8)
        .contains(&raw)
        .then(|| usize::from(raw - ota0))
}

pub(super) fn ota_partition(number: usize) -> Result<AppPartitionSubType, OtaError> {
    let raw = u8::try_from(number)
        .ok()
        .filter(|number| usize::from(*number) < otadata::MAX_OTA_PARTITIONS)
        .ok_or(OtaError::NoBootablePartition)?;
    AppPartitionSubType::try_from(AppPartitionSubType::Ota0 as u8 + raw)
        .map_err(|_| OtaError::NoBootablePartition)
}

pub(super) fn read_entries(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
) -> Result<[SelectEntry; 2], OtaError> {
    let otadata = find_otadata(pt)?;
    let mut region = otadata.as_flash_region(flash);
    let mut read = |offset| -> Result<SelectEntry, OtaError> {
        let mut bytes = [0u8; SLOT_LEN];
        region.read(offset, &mut bytes)?;
        Ok(SelectEntry::decode(&bytes))
    };
    Ok([
        read(otadata::slot_offset(0))?,
        read(otadata::slot_offset(1))?,
    ])
}

/// Writes one select entry. `FlashRegion::write` does a read-modify-write of
/// the sector, so the 4 KiB sector holding the entry is rewritten as a whole
/// and the other sector (the other entry) is never touched.
pub(super) fn write_entry(
    flash: &mut FlashStorage<'_>,
    pt: &PartitionTable<'_>,
    write: EntryWrite,
) -> Result<(), OtaError> {
    let otadata = find_otadata(pt)?;
    otadata
        .as_flash_region(flash)
        .write(write.offset(), &write.entry.encode())?;
    Ok(())
}

fn find_otadata(pt: &PartitionTable<'_>) -> Result<PartitionEntry, OtaError> {
    Ok(pt
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))?
        .ok_or(esp_bootloader_esp_idf::partitions::Error::Invalid)?)
}
