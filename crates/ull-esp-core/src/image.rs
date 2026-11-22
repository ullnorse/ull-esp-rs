//! Checks on the start of an ESP-IDF application image.

pub const APP_IMAGE_MAGIC: u8 = 0xE9;
pub const APP_DESC_MAGIC_OFFSET: usize = 32;
pub const APP_DESC_MAGIC_WORD: u32 = 0xABCD_5432;
/// Bytes needed before [`validate_prefix`] can run.
pub const APP_IMAGE_PREFIX_LEN: usize = APP_DESC_MAGIC_OFFSET + core::mem::size_of::<u32>();
/// Bytes needed before [`app_desc`] can run: the descriptor up to and
/// including its `version` field.
pub const APP_DESC_PREFIX_LEN: usize = APP_DESC_MAGIC_OFFSET + 48;

/// The fields of `esp_app_desc_t` the OTA path needs.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AppDescPrefix<'a> {
    /// Anti-rollback counter. ull-esp-rs firmware sets it to the build time.
    pub secure_version: u32,
    pub version: &'a str,
}

/// Reads the app descriptor from the start of a validated image.
pub fn app_desc(prefix: &[u8]) -> Result<AppDescPrefix<'_>, ImagePrefixError> {
    validate_prefix(prefix)?;
    let Some(prefix) = prefix.get(..APP_DESC_PREFIX_LEN) else {
        return Err(ImagePrefixError::TooShort);
    };
    let desc = &prefix[APP_DESC_MAGIC_OFFSET..];
    let secure_version = u32::from_le_bytes(desc[4..8].try_into().unwrap());
    let version = &desc[16..48];
    let end = version
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(version.len());
    Ok(AppDescPrefix {
        secure_version,
        version: core::str::from_utf8(&version[..end]).unwrap_or(""),
    })
}

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
    fn reads_app_desc() {
        let mut image = [0u8; APP_DESC_PREFIX_LEN];
        image[..APP_IMAGE_PREFIX_LEN].copy_from_slice(&valid_prefix());
        image[36..40].copy_from_slice(&1_795_387_024u32.to_le_bytes());
        image[48..55].copy_from_slice(b"0.2.0+a");
        assert_eq!(
            app_desc(&image),
            Ok(AppDescPrefix {
                secure_version: 1_795_387_024,
                version: "0.2.0+a"
            })
        );
        assert_eq!(app_desc(&image[..60]), Err(ImagePrefixError::TooShort));
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
