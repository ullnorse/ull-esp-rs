use thiserror::Error;

mod boot;
mod image;
mod install;
mod shared;
mod slots;

pub const APP_IMAGE_MAGIC: u8 = 0xE9;
pub const APP_DESC_MAGIC_OFFSET: usize = 32;
pub const APP_DESC_MAGIC_WORD: u32 = 0xABCD5432;
pub const APP_IMAGE_PREFIX_LEN: usize = APP_DESC_MAGIC_OFFSET + core::mem::size_of::<u32>();
pub(crate) const FLASH_SECTOR_SIZE: usize = 4096;
const OTA_DATA_SLOT_LEN: usize = 32;
const OTA_DATA_SECOND_SLOT_OFFSET: u32 = 0x1000;
const OTA_DATA_UNINITIALIZED_SEQUENCE: u32 = u32::MAX;

pub use boot::{
    BootStatus, OtaDebugStatus, OtaSlotDebug, boot_status, bootstrap_factoryless_otadata,
    confirm_running_image, mark_running_state, otadata_debug_status, reject_running_image,
};
pub use image::validate_app_image_prefix;
pub use install::{ExpectedImage, InstalledImage, PartitionWriter, UpdateInstaller};
pub use slots::{UpdateTarget, activate_partition, erase_target_partition, next_update_target};

#[derive(Debug, Error)]
pub enum OtaError {
    #[error("application image is empty")]
    EmptyImage,
    #[error("application image ({0} bytes) does not fit target partition ({1} bytes)")]
    ImageTooLarge(u32, u32),
    #[error("application image ended early: expected {0} bytes, received {1} bytes")]
    IncompleteImage(u32, u32),
    #[error(
        "application image ({0} bytes) is too small to validate required ESP header ({1} bytes)"
    )]
    ImageTooSmallForValidation(u32, u32),
    #[error("invalid ESP image header magic byte: {0:#x}")]
    InvalidImageHeaderMagic(u8),
    #[error("invalid ESP app descriptor magic word: {0:#x}")]
    InvalidAppDescriptorMagic(u32),
    #[error("application image sha256 mismatch")]
    ImageDigestMismatch,
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

impl From<esp_storage::FlashStorageError> for OtaError {
    fn from(value: esp_storage::FlashStorageError) -> Self {
        Self::Flash(value)
    }
}
