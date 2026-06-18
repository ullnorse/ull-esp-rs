use embedded_storage::nor_flash::NorFlash;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::{
    AppPartitionSubType, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionTable,
    PartitionType, read_partition_table,
};
use esp_hal::peripherals::FLASH as Flash;
use esp_storage::FlashStorage;
use thiserror::Error;

pub const APP_IMAGE_MAGIC: u8 = 0xE9;
pub const APP_DESC_MAGIC_OFFSET: usize = 32;
pub const APP_DESC_MAGIC_WORD: u32 = 0xABCD5432;
pub const APP_IMAGE_PREFIX_LEN: usize = APP_DESC_MAGIC_OFFSET + core::mem::size_of::<u32>();
pub const FLASH_SECTOR_SIZE: usize = 4096;
const OTA_DATA_SLOT_LEN: usize = 32;
const OTA_DATA_SECOND_SLOT_OFFSET: u32 = 0x1000;
const OTA_DATA_UNINITIALIZED_SEQUENCE: u32 = u32::MAX;

#[derive(Debug, Error)]
pub enum OtaError {
    #[error("application image is empty")]
    EmptyImage,
    #[error("application image ({0} bytes) does not fit target partition ({1} bytes)")]
    ImageTooLarge(u32, u32),
    #[error("invalid ESP image header magic byte: {0:#x}")]
    InvalidImageHeaderMagic(u8),
    #[error("invalid ESP app descriptor magic word: {0:#x}")]
    InvalidAppDescriptorMagic(u32),
    #[error("booted partition is not an application partition")]
    InvalidBootedPartition,
    #[error("no bootable application partition found")]
    NoBootablePartition,
    #[error("partition table does not contain enough OTA application slots")]
    NotEnoughOtaSlots,
    #[error(transparent)]
    Bootloader(#[from] esp_bootloader_esp_idf::partitions::Error),
    #[error("flash error: {0:?}")]
    Flash(esp_storage::FlashStorageError),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct BootStatus {
    pub booted_partition: Option<AppPartitionSubType>,
    pub selected_partition: Option<AppPartitionSubType>,
    pub ota_state: Option<OtaImageState>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct UpdateTarget {
    pub partition: AppPartitionSubType,
    pub offset: u32,
    pub size: u32,
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

#[repr(C, align(4))]
struct AlignedSectorBuffer {
    bytes: [u8; FLASH_SECTOR_SIZE],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct RawOtaSelectEntry {
    ota_seq: u32,
    ota_state_raw: u32,
    crc: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct RawOtaDataStatus {
    selected_partition: Option<AppPartitionSubType>,
    ota_state: Option<OtaImageState>,
}

pub struct PartitionWriter {
    target: UpdateTarget,
    image_size: u32,
    next_offset: u32,
    received_bytes: u32,
    buffered_bytes: usize,
    buffer: AlignedSectorBuffer,
}

impl PartitionWriter {
    pub fn begin(
        flash: &mut FlashStorage<'_>,
        target: UpdateTarget,
        image_size: u32,
    ) -> Result<Self, OtaError> {
        if image_size == 0 {
            return Err(OtaError::EmptyImage);
        }

        if image_size > target.size {
            return Err(OtaError::ImageTooLarge(image_size, target.size));
        }

        let erase_len = align_up(image_size, FLASH_SECTOR_SIZE as u32);
        NorFlash::erase(flash, target.offset, target.offset + erase_len)?;

        Ok(Self {
            target,
            image_size,
            next_offset: target.offset,
            received_bytes: 0,
            buffered_bytes: 0,
            buffer: AlignedSectorBuffer {
                bytes: [0xff; FLASH_SECTOR_SIZE],
            },
        })
    }

    pub fn bytes_received(&self) -> u32 {
        self.received_bytes
    }

    pub fn write_chunk(
        &mut self,
        flash: &mut FlashStorage<'_>,
        mut chunk: &[u8],
    ) -> Result<(), OtaError> {
        let next_total = self.received_bytes as usize + chunk.len();
        if next_total > self.image_size as usize {
            return Err(OtaError::ImageTooLarge(next_total as u32, self.target.size));
        }

        while !chunk.is_empty() {
            let remaining = FLASH_SECTOR_SIZE - self.buffered_bytes;
            let take = remaining.min(chunk.len());
            self.buffer.bytes[self.buffered_bytes..self.buffered_bytes + take]
                .copy_from_slice(&chunk[..take]);
            self.buffered_bytes += take;
            self.received_bytes += take as u32;
            chunk = &chunk[take..];

            if self.buffered_bytes == FLASH_SECTOR_SIZE {
                self.flush_sector(flash)?;
            }
        }

        Ok(())
    }

    pub fn finish(mut self, flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
        if self.buffered_bytes > 0 {
            self.flush_sector(flash)?;
        }

        Ok(())
    }

    fn flush_sector(&mut self, flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
        NorFlash::write(flash, self.next_offset, &self.buffer.bytes)?;
        self.next_offset += FLASH_SECTOR_SIZE as u32;
        self.buffer.bytes.fill(0xff);
        self.buffered_bytes = 0;
        Ok(())
    }
}

pub type FlashStorageDevice = FlashStorage<'static>;

#[allow(unexpected_cfgs)]
pub fn init_flash_storage(flash: Flash<'static>) -> FlashStorageDevice {
    let flash = FlashStorage::new(flash);

    #[cfg(multi_core)]
    let flash = flash.multicore_auto_park();

    flash
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

    if factory_present {
        return Ok(false);
    }

    if otadata_status.selected_partition.is_some() {
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
        .find_partition(PartitionType::App(next_partition))?
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

pub fn erase_target_partition(
    flash: &mut FlashStorage<'_>,
    target: &UpdateTarget,
) -> Result<(), OtaError> {
    NorFlash::erase(flash, target.offset, target.offset + target.size)?;
    Ok(())
}

pub fn validate_app_image_prefix(prefix: &[u8]) -> Result<(), OtaError> {
    if prefix.len() < APP_IMAGE_PREFIX_LEN {
        return Err(OtaError::EmptyImage);
    }

    if prefix[0] != APP_IMAGE_MAGIC {
        return Err(OtaError::InvalidImageHeaderMagic(prefix[0]));
    }

    let app_desc_magic = u32::from_le_bytes([
        prefix[APP_DESC_MAGIC_OFFSET],
        prefix[APP_DESC_MAGIC_OFFSET + 1],
        prefix[APP_DESC_MAGIC_OFFSET + 2],
        prefix[APP_DESC_MAGIC_OFFSET + 3],
    ]);

    if app_desc_magic != APP_DESC_MAGIC_WORD {
        return Err(OtaError::InvalidAppDescriptorMagic(app_desc_magic));
    }

    Ok(())
}

fn align_up(value: u32, alignment: u32) -> u32 {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

fn app_partition_subtype(
    entry: esp_bootloader_esp_idf::partitions::PartitionEntry<'_>,
) -> Result<AppPartitionSubType, OtaError> {
    match entry.partition_type() {
        PartitionType::App(subtype) => Ok(subtype),
        _ => Err(OtaError::InvalidBootedPartition),
    }
}

fn is_ota_partition(partition: AppPartitionSubType) -> bool {
    partition != AppPartitionSubType::Factory && partition != AppPartitionSubType::Test
}

fn raw_otadata_status(
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

fn write_selected_otadata(
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

fn update_active_otadata_state(
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

fn read_ota_select_entry(
    ota_partition: &mut esp_bootloader_esp_idf::partitions::FlashRegion<'_, FlashStorage<'_>>,
    offset: u32,
) -> Result<RawOtaSelectEntry, OtaError> {
    let mut bytes = [0u8; OTA_DATA_SLOT_LEN];
    embedded_storage::ReadStorage::read(ota_partition, offset, &mut bytes)?;

    Ok(RawOtaSelectEntry {
        ota_seq: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        ota_state_raw: u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        crc: u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
    })
}

fn read_otadata_entries(
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
    embedded_storage::Storage::write(&mut ota_partition, ota_slot_offset(slot), &bytes)?;
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

fn active_otadata_slot(slot0: RawOtaSelectEntry, slot1: RawOtaSelectEntry) -> Option<usize> {
    let slot0_valid = is_valid_otadata_slot(slot0);
    let slot1_valid = is_valid_otadata_slot(slot1);

    match (slot0_valid, slot1_valid) {
        (true, true) => Some(if slot0.ota_seq >= slot1.ota_seq { 0 } else { 1 }),
        (true, false) => Some(0),
        (false, true) => Some(1),
        (false, false) => None,
    }
}

fn is_valid_otadata_slot(slot: RawOtaSelectEntry) -> bool {
    slot.ota_seq != OTA_DATA_UNINITIALIZED_SEQUENCE
        && slot.ota_state_raw != OtaImageState::Invalid as u32
        && slot.ota_state_raw != OtaImageState::Aborted as u32
        && slot.crc == ota_sequence_crc(slot.ota_seq)
}

fn ota_sequence_crc(sequence: u32) -> u32 {
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

fn count_ota_partitions(pt: &PartitionTable<'_>) -> usize {
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

fn next_ota_partition(
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

impl From<esp_storage::FlashStorageError> for OtaError {
    fn from(value: esp_storage::FlashStorageError) -> Self {
        Self::Flash(value)
    }
}
