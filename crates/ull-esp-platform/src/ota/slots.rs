use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::{AppPartitionSubType, PartitionType};
use esp_storage::FlashStorage;
use ull_esp_core::otadata;

use super::OtaError;
use super::shared::{
    layout, ota_number, ota_partition, read_entries, with_partition_table, write_entry,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct UpdateTarget {
    pub partition: AppPartitionSubType,
    pub offset: u32,
    pub size: u32,
}

/// The OTA partition the next update should be written to. Never the running one.
pub fn next_update_target(flash: &mut FlashStorage<'_>) -> Result<UpdateTarget, OtaError> {
    with_partition_table(flash, |flash, pt| {
        let layout = layout(pt)?;
        if layout.ota_partition_count < 2 {
            return Err(OtaError::NotEnoughOtaSlots);
        }

        let entries = read_entries(flash, pt)?;
        let selected = otadata::selected_partition(&entries, layout.ota_partition_count)
            .map(|(number, _)| number);
        let booted = layout.booted.and_then(ota_number);
        let next = ota_partition(otadata::next_update_partition(
            selected,
            booted,
            layout.ota_partition_count,
        ))?;

        let entry = pt
            .find_partition(PartitionType::App(next))?
            .ok_or(OtaError::NoBootablePartition)?;
        Ok(UpdateTarget {
            partition: next,
            offset: entry.offset(),
            size: entry.len(),
        })
    })
}

/// Makes the bootloader boot `partition` next, in `state`.
pub fn activate_partition(
    flash: &mut FlashStorage<'_>,
    partition: AppPartitionSubType,
    state: OtaImageState,
) -> Result<(), OtaError> {
    with_partition_table(flash, |flash, pt| {
        let count = layout(pt)?.ota_partition_count;
        if count < 2 {
            return Err(OtaError::NotEnoughOtaSlots);
        }
        let number = ota_number(partition)
            .filter(|number| *number < count)
            .ok_or(OtaError::NoBootablePartition)?;

        let entries = read_entries(flash, pt)?;
        let write = otadata::select_partition(&entries, number, count, state as u32);
        write_entry(flash, pt, write)
    })
}
