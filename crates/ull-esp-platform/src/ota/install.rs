use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::AppPartitionSubType;
use esp_storage::FlashStorage;
use sha2::{Digest, Sha256};
use ull_esp_core::image::{APP_DESC_PREFIX_LEN, app_desc};
use ull_esp_core::signature::{PUBLIC_KEY_LEN, SIGNATURE_LEN, verify_image};

use crate::flash::FlashStorageDevice;

use super::slots::{UpdateTarget, activate_partition, next_update_target};
use super::{FLASH_SECTOR_SIZE, OtaError};

const SECTOR: usize = FLASH_SECTOR_SIZE as usize;

#[repr(C, align(4))]
struct AlignedSectorBuffer {
    bytes: [u8; SECTOR],
}

/// What the server claims about an update, checked before and after download.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ExpectedImage {
    pub size: u32,
    pub sha256: [u8; 32],
    /// Ed25519 signature over the digest (see `ull_esp_core::signature`).
    pub signature: Option<[u8; SIGNATURE_LEN]>,
}

impl ExpectedImage {
    pub const fn new(size: u32, sha256: [u8; 32]) -> Self {
        Self {
            size,
            sha256,
            signature: None,
        }
    }

    pub const fn with_signature(mut self, signature: [u8; SIGNATURE_LEN]) -> Self {
        self.signature = Some(signature);
        self
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstalledImage {
    pub partition: AppPartitionSubType,
    pub size: u32,
}

/// Streams an image into a partition one sector at a time. Each sector is
/// erased right before it is written, so a slow download never leaves a long
/// erased gap and a small image never erases the whole slot.
pub struct PartitionWriter {
    target: UpdateTarget,
    image_size: u32,
    next_offset: u32,
    received_bytes: u32,
    buffered_bytes: usize,
    buffer: AlignedSectorBuffer,
}

impl PartitionWriter {
    pub fn begin(target: UpdateTarget, image_size: u32) -> Result<Self, OtaError> {
        if image_size == 0 {
            return Err(OtaError::EmptyImage);
        }

        if image_size > target.size {
            return Err(OtaError::ImageTooLarge(image_size, target.size));
        }

        Ok(Self {
            target,
            image_size,
            next_offset: target.offset,
            received_bytes: 0,
            buffered_bytes: 0,
            buffer: AlignedSectorBuffer {
                bytes: [0xff; SECTOR],
            },
        })
    }

    pub fn bytes_received(&self) -> u32 {
        self.received_bytes
    }

    /// Whether any sector of the target partition has been erased or written.
    pub fn touched_flash(&self) -> bool {
        self.next_offset != self.target.offset
    }

    pub fn write_chunk(
        &mut self,
        flash: &mut FlashStorage<'_>,
        mut chunk: &[u8],
    ) -> Result<(), OtaError> {
        let next_total = self.received_bytes as usize + chunk.len();
        if next_total > self.image_size as usize {
            return Err(OtaError::ImageOverrun(self.image_size));
        }

        while !chunk.is_empty() {
            let take = (SECTOR - self.buffered_bytes).min(chunk.len());
            self.buffer.bytes[self.buffered_bytes..self.buffered_bytes + take]
                .copy_from_slice(&chunk[..take]);
            self.buffered_bytes += take;
            self.received_bytes += take as u32;
            chunk = &chunk[take..];

            if self.buffered_bytes == SECTOR {
                self.flush_sector(flash)?;
            }
        }

        Ok(())
    }

    pub fn finish(&mut self, flash: &mut FlashStorage<'_>) -> Result<(), OtaError> {
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
        let offset = self.next_offset;
        // Advance first: if the erase fails half-way the sector still counts as
        // touched, so cleanup runs.
        self.next_offset += FLASH_SECTOR_SIZE;
        flash.erase(offset, offset + FLASH_SECTOR_SIZE)?;
        flash.write_nor(offset, &self.buffer.bytes)?;
        self.buffer.bytes.fill(0xff);
        self.buffered_bytes = 0;
        Ok(())
    }
}

/// Downloads, checks and activates one update.
///
/// Order of checks:
/// 1. `begin`: size fits, and the signature over the claimed digest verifies
///    (when a public key is given). Nothing is erased if this fails.
/// 2. `write_chunk`: the ESP image header and app descriptor (magic words and
///    the anti-rollback `secure_version`), before the first sector is erased.
/// 3. `finish`: byte count and the SHA-256 of what was written equals the
///    signed digest. Only then is otadata switched to the new partition.
///
/// Dropping the installer before `finish` succeeds erases the target's first
/// sector, so a partial image can never pass the bootloader's header check.
pub struct UpdateInstaller<'a> {
    flash: &'a mut FlashStorageDevice,
    expected: ExpectedImage,
    target: UpdateTarget,
    writer: PartitionWriter,
    prefix: [u8; APP_DESC_PREFIX_LEN],
    prefix_len: usize,
    min_secure_version: u32,
    hasher: Sha256,
    activated: bool,
}

impl<'a> UpdateInstaller<'a> {
    /// `public_key`: `Some` requires `expected.signature` to verify; `None`
    /// accepts any image whose digest matches (integrity only, no authenticity).
    pub fn begin(
        flash: &'a mut FlashStorageDevice,
        expected: ExpectedImage,
        public_key: Option<&[u8; PUBLIC_KEY_LEN]>,
    ) -> Result<Self, OtaError> {
        if let Some(public_key) = public_key {
            let signature = expected.signature.ok_or(OtaError::MissingSignature)?;
            verify_image(public_key, &expected.sha256, &signature)?;
        }

        let target = next_update_target(flash)?;
        let writer = PartitionWriter::begin(target, expected.size)?;

        Ok(Self {
            flash,
            expected,
            target,
            writer,
            prefix: [0u8; APP_DESC_PREFIX_LEN],
            prefix_len: 0,
            min_secure_version: 0,
            hasher: Sha256::new(),
            activated: false,
        })
    }

    /// Rejects images whose app descriptor `secure_version` is below `min`,
    /// before anything is erased. Pass the running firmware's own
    /// `secure_version` so a replayed older (but validly signed) image is refused.
    pub fn require_secure_version(mut self, min: u32) -> Self {
        self.min_secure_version = min;
        self
    }

    pub fn target(&self) -> UpdateTarget {
        self.target
    }

    pub fn bytes_received(&self) -> u32 {
        self.writer.bytes_received()
    }

    pub fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), OtaError> {
        if self.prefix_len < APP_DESC_PREFIX_LEN {
            let take = (APP_DESC_PREFIX_LEN - self.prefix_len).min(chunk.len());
            self.prefix[self.prefix_len..self.prefix_len + take].copy_from_slice(&chunk[..take]);
            self.prefix_len += take;

            if self.prefix_len == APP_DESC_PREFIX_LEN {
                let desc = app_desc(&self.prefix)?;
                if desc.secure_version < self.min_secure_version {
                    return Err(OtaError::Rollback {
                        image: desc.secure_version,
                        running: self.min_secure_version,
                    });
                }
            }
        }

        self.hasher.update(chunk);
        self.writer.write_chunk(self.flash, chunk)
    }

    pub fn finish(mut self) -> Result<InstalledImage, OtaError> {
        if self.prefix_len < APP_DESC_PREFIX_LEN {
            return Err(OtaError::ImageTooSmallForValidation);
        }

        self.writer.finish(self.flash)?;

        let digest: [u8; 32] = core::mem::take(&mut self.hasher).finalize().into();
        if digest != self.expected.sha256 {
            return Err(OtaError::ImageDigestMismatch);
        }

        activate_partition(self.flash, self.target.partition, OtaImageState::New)?;
        self.activated = true;

        Ok(InstalledImage {
            partition: self.target.partition,
            size: self.expected.size,
        })
    }
}

impl Drop for UpdateInstaller<'_> {
    fn drop(&mut self) {
        if self.activated || !self.writer.touched_flash() {
            return;
        }

        let start = self.target.offset;
        if let Err(err) = self.flash.erase(start, start + FLASH_SECTOR_SIZE) {
            log::warn!("failed to erase OTA target header after aborted install: {err:?}");
        }
    }
}
