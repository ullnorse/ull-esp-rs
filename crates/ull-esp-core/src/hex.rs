//! Fixed-length hex decoding for digests, signatures and keys.

/// Decodes exactly `2 * N` hex digits (either case) into `N` bytes.
pub fn decode<const N: usize>(value: &str) -> Option<[u8; N]> {
    let digits = value.as_bytes();
    if digits.len() != N * 2 {
        return None;
    }

    let mut bytes = [0u8; N];
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, pair) in bytes.iter_mut().zip(pairs) {
        *byte = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(bytes)
}

/// Like [`decode`], but usable in `const` context so build-time keys fail to
/// compile instead of failing at runtime.
pub const fn decode_const<const N: usize>(value: &str) -> [u8; N] {
    let digits = value.as_bytes();
    assert!(digits.len() == N * 2, "hex string has the wrong length");

    let mut bytes = [0u8; N];
    let mut index = 0;
    while index < N {
        let (Some(high), Some(low)) = (nibble(digits[index * 2]), nibble(digits[index * 2 + 1]))
        else {
            panic!("hex string contains a non-hex character");
        };
        bytes[index] = (high << 4) | low;
        index += 1;
    }
    bytes
}

const fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_mixed_case() {
        assert_eq!(decode::<3>("00aBFf"), Some([0x00, 0xab, 0xff]));
    }

    #[test]
    fn rejects_wrong_length_and_bad_digits() {
        assert_eq!(decode::<2>("abc"), None);
        assert_eq!(decode::<2>("abcdef"), None);
        assert_eq!(decode::<2>("zz00"), None);
        assert_eq!(decode::<2>("+1ff"), None);
    }

    #[test]
    fn const_decode_matches_runtime_decode() {
        const KEY: [u8; 4] = decode_const("deadBEEF");
        assert_eq!(Some(KEY), decode::<4>("deadbeef"));
    }
}
