use embedded_storage::nor_flash::NorFlash;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::AppPartitionSubType;
use esp_storage::FlashStorage;
use sha2::{Digest, Sha256};

use crate::flash::FlashStorageDevice;

use super::shared::align_up;
use super::slots::{UpdateTarget, activate_partition, erase_target_partition, next_update_target};
use super::{APP_IMAGE_PREFIX_LEN, FLASH_SECTOR_SIZE, OtaError, validate_app_image_prefix};

#[repr(C, align(4))]
struct AlignedSectorBuffer {
    bytes: [u8; FLASH_SECTOR_SIZE],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ExpectedImage {
    pub size: u32,
    pub sha256: [u8; 32],
}

impl ExpectedImage {
    pub const fn new(size: u32, sha256: [u8; 32]) -> Self {
        Self { size, sha256 }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstalledImage {
    pub partition: AppPartitionSubType,
    pub size: u32,
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
        if self.received_bytes != self.image_size {
            return Err(OtaError::IncompleteImage(
                self.image_size,
                self.received_bytes,
            ));
        }

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

pub struct UpdateInstaller<'a> {
    flash: &'a mut FlashStorageDevice,
    expected: ExpectedImage,
    target: UpdateTarget,
    writer: Option<PartitionWriter>,
    prefix: [u8; APP_IMAGE_PREFIX_LEN],
    prefix_len: usize,
    prefix_checked: bool,
    hasher: Sha256,
    cleanup_required: bool,
}

impl<'a> UpdateInstaller<'a> {
    pub fn begin(
        flash: &'a mut FlashStorageDevice,
        expected: ExpectedImage,
    ) -> Result<Self, OtaError> {
        let target = next_update_target(flash)?;
        let writer = PartitionWriter::begin(flash, target, expected.size)?;

        Ok(Self {
            flash,
            expected,
            target,
            writer: Some(writer),
            prefix: [0u8; APP_IMAGE_PREFIX_LEN],
            prefix_len: 0,
            prefix_checked: false,
            hasher: Sha256::new(),
            cleanup_required: true,
        })
    }

    pub fn target(&self) -> UpdateTarget {
        self.target
    }

    pub fn bytes_received(&self) -> u32 {
        self.writer
            .as_ref()
            .expect("OTA installer writer missing")
            .bytes_received()
    }

    pub fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), OtaError> {
        let prefix_remaining = self.prefix.len().saturating_sub(self.prefix_len);
        if prefix_remaining > 0 {
            let take = prefix_remaining.min(chunk.len());
            self.prefix[self.prefix_len..self.prefix_len + take].copy_from_slice(&chunk[..take]);
            self.prefix_len += take;

            if !self.prefix_checked && self.prefix_len == self.prefix.len() {
                validate_app_image_prefix(&self.prefix)?;
                self.prefix_checked = true;
            }
        }

        self.hasher.update(chunk);
        self.writer
            .as_mut()
            .expect("OTA installer writer missing")
            .write_chunk(self.flash, chunk)?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<InstalledImage, OtaError> {
        if self.bytes_received() != self.expected.size {
            return Err(OtaError::IncompleteImage(
                self.expected.size,
                self.bytes_received(),
            ));
        }

        if !self.prefix_checked {
            return Err(OtaError::ImageTooSmallForValidation(
                self.bytes_received(),
                APP_IMAGE_PREFIX_LEN as u32,
            ));
        }

        self.writer
            .take()
            .expect("OTA installer writer missing")
            .finish(self.flash)?;

        let actual_digest: [u8; 32] = core::mem::take(&mut self.hasher).finalize().into();
        if actual_digest != self.expected.sha256 {
            return Err(OtaError::ImageDigestMismatch);
        }

        activate_partition(self.flash, self.target.partition, OtaImageState::New)?;
        self.cleanup_required = false;

        Ok(InstalledImage {
            partition: self.target.partition,
            size: self.expected.size,
        })
    }
}

impl Drop for UpdateInstaller<'_> {
    fn drop(&mut self) {
        if !self.cleanup_required {
            return;
        }

        if let Err(err) = erase_target_partition(self.flash, &self.target) {
            log::warn!("failed to erase OTA target after aborted install: {err}");
        }
    }
}
