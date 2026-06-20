use super::OtaError;
use super::{APP_DESC_MAGIC_OFFSET, APP_DESC_MAGIC_WORD, APP_IMAGE_MAGIC, APP_IMAGE_PREFIX_LEN};

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
