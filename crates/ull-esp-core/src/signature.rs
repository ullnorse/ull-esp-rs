//! Ed25519 signatures over OTA images.
//!
//! The developer signs `SIGNATURE_CONTEXT || sha256(image)` offline with
//! `ull-fleet-sign`. The device streams the image, hashes it, and checks the
//! signature against a public key built into the firmware before it activates
//! the new partition. The fleet server only stores and forwards the signature,
//! so a compromised server cannot push an image the device will boot.

use ed25519_dalek::{Signature, VerifyingKey};

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
/// Domain separation, so an OTA signature cannot be reused for anything else.
/// `ull-fleet-sign` uses the same bytes.
pub const SIGNATURE_CONTEXT: &[u8; 20] = b"ull-esp-ota-image-v1";
pub const SIGNED_MESSAGE_LEN: usize = SIGNATURE_CONTEXT.len() + 32;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SignatureError {
    InvalidPublicKey,
    BadSignature,
}

/// The exact bytes that get signed for an image with this SHA-256 digest.
pub fn signed_message(sha256: &[u8; 32]) -> [u8; SIGNED_MESSAGE_LEN] {
    let mut message = [0u8; SIGNED_MESSAGE_LEN];
    message[..SIGNATURE_CONTEXT.len()].copy_from_slice(SIGNATURE_CONTEXT);
    message[SIGNATURE_CONTEXT.len()..].copy_from_slice(sha256);
    message
}

/// Checks `signature` over an image digest. Uses strict verification, which
/// rejects weak public keys and non-canonical signatures.
pub fn verify_image(
    public_key: &[u8; PUBLIC_KEY_LEN],
    sha256: &[u8; 32],
    signature: &[u8; SIGNATURE_LEN],
) -> Result<(), SignatureError> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| SignatureError::InvalidPublicKey)?;
    if key.is_weak() {
        return Err(SignatureError::InvalidPublicKey);
    }
    key.verify_strict(&signed_message(sha256), &Signature::from_bytes(signature))
        .map_err(|_| SignatureError::BadSignature)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;
    use crate::hex;

    const SEED: [u8; 32] = [7; 32];
    const DIGEST: [u8; 32] = [0xab; 32];

    fn sign(seed: &[u8; 32], digest: &[u8; 32]) -> ([u8; 32], [u8; 64]) {
        let key = SigningKey::from_bytes(seed);
        (
            key.verifying_key().to_bytes(),
            key.sign(&signed_message(digest)).to_bytes(),
        )
    }

    #[test]
    fn accepts_valid_signature() {
        let (public_key, signature) = sign(&SEED, &DIGEST);
        assert_eq!(verify_image(&public_key, &DIGEST, &signature), Ok(()));
    }

    #[test]
    fn rejects_other_digest_key_or_tampered_signature() {
        let (public_key, mut signature) = sign(&SEED, &DIGEST);
        let (other_key, _) = sign(&[8; 32], &DIGEST);

        assert_eq!(
            verify_image(&public_key, &[0xac; 32], &signature),
            Err(SignatureError::BadSignature)
        );
        assert_eq!(
            verify_image(&other_key, &DIGEST, &signature),
            Err(SignatureError::BadSignature)
        );
        signature[0] ^= 1;
        assert_eq!(
            verify_image(&public_key, &DIGEST, &signature),
            Err(SignatureError::BadSignature)
        );
    }

    #[test]
    fn rejects_signature_without_context() {
        let key = SigningKey::from_bytes(&SEED);
        let signature = key.sign(&DIGEST).to_bytes();
        assert_eq!(
            verify_image(&key.verifying_key().to_bytes(), &DIGEST, &signature),
            Err(SignatureError::BadSignature)
        );
    }

    #[test]
    fn rejects_weak_public_key() {
        // The identity point is a valid encoding but a weak key.
        let mut identity = [0u8; 32];
        identity[0] = 1;
        let (_, signature) = sign(&SEED, &DIGEST);
        assert_eq!(
            verify_image(&identity, &DIGEST, &signature),
            Err(SignatureError::InvalidPublicKey)
        );
    }

    /// Shared with `ull-fleet-sign`'s tests so both sides sign the same bytes.
    #[test]
    fn cross_repo_test_vector() {
        let (public_key, signature) = sign(&SEED, &DIGEST);
        assert_eq!(
            public_key,
            hex::decode::<32>(TEST_VECTOR_PUBLIC_KEY).unwrap(),
            "public key changed"
        );
        assert_eq!(
            signature,
            hex::decode::<64>(TEST_VECTOR_SIGNATURE).unwrap(),
            "signature changed"
        );
    }

    // Generated independently with Python `cryptography`:
    // Ed25519PrivateKey.from_private_bytes(bytes([7] * 32)) signing
    // b"ull-esp-ota-image-v1" + bytes([0xab] * 32).
    const TEST_VECTOR_PUBLIC_KEY: &str =
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
    const TEST_VECTOR_SIGNATURE: &str = concat!(
        "c1c2fa02a0e90648084724d2ad795a73132e04f18cebe93f1e690135382afcc9",
        "5a6c2ac63b3e34b389735177051775a03b6e1c18d7cf53d57e9d8a15d300b502",
    );
}
