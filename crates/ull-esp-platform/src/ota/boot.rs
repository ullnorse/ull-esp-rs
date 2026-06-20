use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::{
    AppPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType, read_partition_table,
};
use esp_storage::FlashStorage;

use super::OtaError;
use super::shared::{
    RawOtaSelectEntry, active_otadata_slot, app_partition_subtype, count_ota_partitions,
    is_ota_partition, is_valid_otadata_slot, ota_sequence_crc, raw_otadata_status,
    read_otadata_entries, update_active_otadata_state, write_selected_otadata,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct BootStatus {
    pub booted_partition: Option<AppPartitionSubType>,
    pub selected_partition: Option<AppPartitionSubType>,
    pub ota_state: Option<OtaImageState>,
}

impl BootStatus {
    pub fn requires_health_confirmation(self) -> bool {
        matches!(
            self.ota_state,
            Some(OtaImageState::PendingVerify | OtaImageState::New)
        )
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct OtaSlotDebug {
    pub sequence: u32,
    pub state_raw: u32,
    pub state: Option<OtaImageState>,
    pub crc: u32,
    pub expected_crc: u32,
    pub valid: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct OtaDebugStatus {
    pub slot0: OtaSlotDebug,
    pub slot1: OtaSlotDebug,
    pub active_slot: Option<usize>,
}

pub fn boot_status(flash: &mut FlashStorage<'_>) -> Result<BootStatus, OtaError> {
    let (booted_partition, otadata_status) = {
        let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
        let pt = read_partition_table(flash, &mut pt_buffer)?;
        let ota_partition_count = count_ota_partitions(&pt);

        let booted_partition = pt
            .booted_partition()?
            .map(app_partition_subtype)
            .transpose()?;
        let otadata_status = raw_otadata_status(flash, &pt, ota_partition_count)?;

        (booted_partition, otadata_status)
    };

    Ok(BootStatus {
        booted_partition,
        selected_partition: otadata_status.selected_partition,
        ota_state: otadata_status.ota_state,
    })
}

pub fn bootstrap_factoryless_otadata(flash: &mut FlashStorage<'_>) -> Result<bool, OtaError> {
    let (booted_partition, factory_present, otadata_status, ota_partition_count) = {
        let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
        let pt = read_partition_table(flash, &mut pt_buffer)?;
        let ota_partition_count = count_ota_partitions(&pt);

        let booted_partition = pt
            .booted_partition()?
            .map(app_partition_subtype)
            .transpose()?;
        let factory_present = pt
            .find_partition(PartitionType::App(AppPartitionSubType::Factory))?
            .is_some();
        let otadata_status = raw_otadata_status(flash, &pt, ota_partition_count)?;

        (
            booted_partition,
            factory_present,
            otadata_status,
            ota_partition_count,
        )
    };

    if factory_present || otadata_status.selected_partition.is_some() {
        return Ok(false);
    }

    let booted_partition = booted_partition.ok_or(OtaError::NoBootablePartition)?;
    if !is_ota_partition(booted_partition) {
        return Err(OtaError::NoBootablePartition);
    }

    let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    let pt = read_partition_table(flash, &mut pt_buffer)?;
    write_selected_otadata(
        flash,
        &pt,
        ota_partition_count,
        booted_partition,
        OtaImageState::Valid,
    )?;

    Ok(true)
}

pub fn confirm_running_image(flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
    mark_running_state(flash, OtaImageState::Valid)
}

pub fn reject_running_image(flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
    mark_running_state(flash, OtaImageState::Invalid)
}

pub fn mark_running_state(
    flash: &mut FlashStorage<'_>,
    state: OtaImageState,
) -> Result<(), OtaError> {
    let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    let pt = read_partition_table(flash, &mut pt_buffer)?;
    let ota_partition_count = count_ota_partitions(&pt);
    update_active_otadata_state(flash, &pt, ota_partition_count, state)?;
    Ok(())
}

pub fn otadata_debug_status(flash: &mut FlashStorage<'_>) -> Result<OtaDebugStatus, OtaError> {
    let mut pt_buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    let pt = read_partition_table(flash, &mut pt_buffer)?;
    let (slot0, slot1) = read_otadata_entries(flash, &pt)?;

    Ok(OtaDebugStatus {
        slot0: ota_slot_debug(slot0),
        slot1: ota_slot_debug(slot1),
        active_slot: active_otadata_slot(slot0, slot1),
    })
}

fn ota_slot_debug(slot: RawOtaSelectEntry) -> OtaSlotDebug {
    OtaSlotDebug {
        sequence: slot.ota_seq,
        state_raw: slot.ota_state_raw,
        state: OtaImageState::try_from(slot.ota_state_raw).ok(),
        crc: slot.crc,
        expected_crc: ota_sequence_crc(slot.ota_seq),
        valid: is_valid_otadata_slot(slot),
    }
}
