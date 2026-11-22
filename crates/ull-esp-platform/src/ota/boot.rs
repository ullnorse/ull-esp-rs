use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::AppPartitionSubType;
use esp_storage::FlashStorage;
use ull_esp_core::otadata::{self, SelectEntry};

use super::OtaError;
use super::shared::{
    layout, ota_number, ota_partition, read_entries, with_partition_table, write_entry,
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
    with_partition_table(flash, |flash, pt| {
        let layout = layout(pt)?;
        let selected = if layout.ota_partition_count == 0 {
            None
        } else {
            otadata::selected_partition(&read_entries(flash, pt)?, layout.ota_partition_count)
        };

        Ok(BootStatus {
            booted_partition: layout.booted,
            selected_partition: selected
                .map(|(number, _)| ota_partition(number))
                .transpose()?,
            ota_state: selected.and_then(|(_, state)| OtaImageState::try_from(state).ok()),
        })
    })
}

/// On a layout without a factory partition and with blank otadata (fresh
/// flash), marks the running OTA partition as selected and valid. Returns
/// whether it wrote anything.
pub fn bootstrap_factoryless_otadata(flash: &mut FlashStorage<'_>) -> Result<bool, OtaError> {
    with_partition_table(flash, |flash, pt| {
        let layout = layout(pt)?;
        if layout.factory_present || layout.ota_partition_count == 0 {
            return Ok(false);
        }

        let entries = read_entries(flash, pt)?;
        if otadata::active_slot(&entries).is_some() {
            return Ok(false);
        }

        let booted = layout
            .booted
            .and_then(ota_number)
            .ok_or(OtaError::NoBootablePartition)?;
        let write = otadata::select_partition(
            &entries,
            booted,
            layout.ota_partition_count,
            OtaImageState::Valid as u32,
        );
        write_entry(flash, pt, write)?;
        Ok(true)
    })
}

pub fn confirm_running_image(flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
    mark_running_state(flash, OtaImageState::Valid)
}

pub fn reject_running_image(flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
    mark_running_state(flash, OtaImageState::Invalid)
}

/// Sets the state of the active otadata entry.
pub fn mark_running_state(
    flash: &mut FlashStorage<'_>,
    state: OtaImageState,
) -> Result<(), OtaError> {
    with_partition_table(flash, |flash, pt| {
        let entries = read_entries(flash, pt)?;
        let write = otadata::set_active_state(&entries, state as u32)
            .ok_or(esp_bootloader_esp_idf::partitions::Error::InvalidState)?;
        write_entry(flash, pt, write)
    })
}

pub fn otadata_debug_status(flash: &mut FlashStorage<'_>) -> Result<OtaDebugStatus, OtaError> {
    with_partition_table(flash, |flash, pt| {
        let entries = read_entries(flash, pt)?;
        Ok(OtaDebugStatus {
            slot0: slot_debug(entries[0]),
            slot1: slot_debug(entries[1]),
            active_slot: otadata::active_slot(&entries),
        })
    })
}

fn slot_debug(entry: SelectEntry) -> OtaSlotDebug {
    OtaSlotDebug {
        sequence: entry.sequence,
        state_raw: entry.state,
        state: OtaImageState::try_from(entry.state).ok(),
        crc: entry.crc,
        expected_crc: otadata::sequence_crc(entry.sequence),
        valid: entry.is_valid(),
    }
}
