use embedded_storage::nor_flash::NorFlash;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::{
    AppPartitionSubType, PARTITION_TABLE_MAX_LEN, read_partition_table,
};
use esp_storage::FlashStorage;

use super::OtaError;
use super::shared::{
    app_partition_subtype, count_ota_partitions, next_ota_partition, raw_otadata_status,
    write_selected_otadata,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct UpdateTarget {
    pub partition: AppPartitionSubType,
    pub offset: u32,
    pub size: u32,
}

pub fn next_update_target(flash: &mut FlashStorage<'_>) -> Result<UpdateTarget, OtaError> {
    let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    let pt = read_partition_table(flash, &mut pt_buffer)?;
    let ota_partition_count = count_ota_partitions(&pt);

    if ota_partition_count < 2 {
        return Err(OtaError::NotEnoughOtaSlots);
    }

    let booted_partition = pt
        .booted_partition()?
        .map(app_partition_subtype)
        .transpose()?;
    let selected_partition = raw_otadata_status(flash, &pt, ota_partition_count)?
        .selected_partition
        .or(booted_partition)
        .ok_or(OtaError::NoBootablePartition)?;
    let next_partition =
        next_ota_partition(selected_partition, booted_partition, ota_partition_count)?;
    let target_entry = pt
        .find_partition(esp_bootloader_esp_idf::partitions::PartitionType::App(
            next_partition,
        ))?
        .ok_or(OtaError::NoBootablePartition)?;

    Ok(UpdateTarget {
        partition: next_partition,
        offset: target_entry.offset(),
        size: target_entry.len(),
    })
}

pub fn activate_partition(
    flash: &mut FlashStorage<'_>,
    partition: AppPartitionSubType,
    state: OtaImageState,
) -> Result<(), OtaError> {
    let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    let pt = read_partition_table(flash, &mut pt_buffer)?;
    let ota_partition_count = count_ota_partitions(&pt);

    if ota_partition_count < 2 {
        return Err(OtaError::NotEnoughOtaSlots);
    }

    write_selected_otadata(flash, &pt, ota_partition_count, partition, state)?;
    Ok(())
}

pub fn erase_target_partition(
    flash: &mut FlashStorage<'_>,
    target: &UpdateTarget,
) -> Result<(), OtaError> {
    NorFlash::erase(flash, target.offset, target.offset + target.size)?;
    Ok(())
}
