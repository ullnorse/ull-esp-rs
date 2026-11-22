//! Checks on the start of an ESP-IDF application image.

pub const APP_IMAGE_MAGIC: u8 = 0xE9;
pub const APP_DESC_MAGIC_OFFSET: usize = 32;
pub const APP_DESC_MAGIC_WORD: u32 = 0xABCD_5432;
/// Bytes needed before [`validate_prefix`] can run.
pub const APP_IMAGE_PREFIX_LEN: usize = APP_DESC_MAGIC_OFFSET + core::mem::size_of::<u32>();

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ImagePrefixError {
    TooShort,
    InvalidHeaderMagic(u8),
    InvalidAppDescriptorMagic(u32),
}

/// Checks the image header magic byte and the app descriptor magic word that
/// follows the first segment header.
pub fn validate_prefix(prefix: &[u8]) -> Result<(), ImagePrefixError> {
    let Some(prefix) = prefix.get(..APP_IMAGE_PREFIX_LEN) else {
        return Err(ImagePrefixError::TooShort);
    };

    if prefix[0] != APP_IMAGE_MAGIC {
        return Err(ImagePrefixError::InvalidHeaderMagic(prefix[0]));
    }

    let word: [u8; 4] = prefix[APP_DESC_MAGIC_OFFSET..].try_into().unwrap();
    let app_desc_magic = u32::from_le_bytes(word);
    if app_desc_magic != APP_DESC_MAGIC_WORD {
        return Err(ImagePrefixError::InvalidAppDescriptorMagic(app_desc_magic));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_prefix() -> [u8; APP_IMAGE_PREFIX_LEN] {
        let mut prefix = [0u8; APP_IMAGE_PREFIX_LEN];
        prefix[0] = APP_IMAGE_MAGIC;
        prefix[APP_DESC_MAGIC_OFFSET..].copy_from_slice(&APP_DESC_MAGIC_WORD.to_le_bytes());
        prefix
    }

    #[test]
    fn accepts_valid_prefix() {
        assert_eq!(validate_prefix(&valid_prefix()), Ok(()));
    }

    #[test]
    fn rejects_short_prefix() {
        assert_eq!(
            validate_prefix(&valid_prefix()[..APP_IMAGE_PREFIX_LEN - 1]),
            Err(ImagePrefixError::TooShort)
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let mut prefix = valid_prefix();
        prefix[0] = 0x7f;
        assert_eq!(
            validate_prefix(&prefix),
            Err(ImagePrefixError::InvalidHeaderMagic(0x7f))
        );

        let mut prefix = valid_prefix();
        prefix[APP_DESC_MAGIC_OFFSET] = 0;
        assert_eq!(
            validate_prefix(&prefix),
            Err(ImagePrefixError::InvalidAppDescriptorMagic(0xABCD_5400))
        );
    }
}
