#![no_std]

pub mod config;
pub mod error;
pub mod flash;
pub mod fleet;
pub mod http;
pub mod i2c;
pub mod ota;
pub mod runtime;
pub mod spi;
pub mod uart;
pub mod wifi;

pub use ull_esp_core::hex;

#[doc(hidden)]
pub use esp_bootloader_esp_idf as __bootloader;

/// Defines the ESP app descriptor from the release identity that
/// `examples/build_support.rs` (`export_firmware_identity`) exports:
/// version string, build date/time and anti-rollback `secure_version`.
#[macro_export]
macro_rules! firmware_app_desc {
    () => {
        $crate::__bootloader::esp_app_desc!(
            env!("FIRMWARE_VERSION"),
            env!("CARGO_PKG_NAME"),
            env!("FIRMWARE_BUILD_TIME"),
            env!("FIRMWARE_BUILD_DATE"),
            $crate::__bootloader::ESP_IDF_COMPATIBLE_VERSION,
            $crate::__bootloader::MMU_PAGE_SIZE,
            0,
            u16::MAX,
            $crate::parse_u32(env!("FIRMWARE_SECURE_VERSION"))
        );
    };
}

/// Compile-time decimal parse for [`firmware_app_desc!`].
#[doc(hidden)]
pub const fn parse_u32(value: &str) -> u32 {
    let bytes = value.as_bytes();
    assert!(!bytes.is_empty(), "empty number");
    let mut result: u32 = 0;
    let mut index = 0;
    while index < bytes.len() {
        assert!(bytes[index].is_ascii_digit(), "not a decimal number");
        result = result * 10 + (bytes[index] - b'0') as u32;
        index += 1;
    }
    result
}
