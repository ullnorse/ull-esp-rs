use thiserror::Error;
use ull_esp_core::image::ImagePrefixError;
use ull_esp_core::signature::SignatureError;

mod boot;
mod install;
mod shared;
mod slots;

pub(crate) const FLASH_SECTOR_SIZE: u32 = 4096;

pub use boot::{
    BootStatus, OtaDebugStatus, OtaSlotDebug, boot_status, bootstrap_factoryless_otadata,
    confirm_running_image, mark_running_state, otadata_debug_status, reject_running_image,
};
pub use install::{ExpectedImage, InstalledImage, PartitionWriter, UpdateInstaller};
pub use slots::{UpdateTarget, activate_partition, next_update_target};
pub use ull_esp_core::image::{APP_IMAGE_PREFIX_LEN, validate_prefix as validate_app_image_prefix};
pub use ull_esp_core::signature::{PUBLIC_KEY_LEN, SIGNATURE_LEN};

#[derive(Debug, Error)]
pub enum OtaError {
    #[error("application image is empty")]
    EmptyImage,
    #[error("application image ({0} bytes) does not fit target partition ({1} bytes)")]
    ImageTooLarge(u32, u32),
    #[error("application image ended early: expected {0} bytes, received {1} bytes")]
    IncompleteImage(u32, u32),
    #[error("application image sent more than the expected {0} bytes")]
    ImageOverrun(u32),
    #[error("application image is shorter than the {APP_IMAGE_PREFIX_LEN}-byte ESP header")]
    ImageTooSmallForValidation,
    #[error("invalid ESP image header magic byte: {0:#x}")]
    InvalidImageHeaderMagic(u8),
    #[error("invalid ESP app descriptor magic word: {0:#x}")]
    InvalidAppDescriptorMagic(u32),
    #[error("application image sha256 mismatch")]
    ImageDigestMismatch,
    #[error("update is not signed, but a public key is configured")]
    MissingSignature,
    #[error("update signature does not verify against the configured public key")]
    BadSignature,
    #[error("configured OTA public key is not a valid Ed25519 key")]
    InvalidPublicKey,
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

impl From<ImagePrefixError> for OtaError {
    fn from(value: ImagePrefixError) -> Self {
        match value {
            ImagePrefixError::TooShort => Self::ImageTooSmallForValidation,
            ImagePrefixError::InvalidHeaderMagic(magic) => Self::InvalidImageHeaderMagic(magic),
            ImagePrefixError::InvalidAppDescriptorMagic(magic) => {
                Self::InvalidAppDescriptorMagic(magic)
            }
        }
    }
}

impl From<SignatureError> for OtaError {
    fn from(value: SignatureError) -> Self {
        match value {
            SignatureError::InvalidPublicKey => Self::InvalidPublicKey,
            SignatureError::BadSignature => Self::BadSignature,
        }
    }
}
