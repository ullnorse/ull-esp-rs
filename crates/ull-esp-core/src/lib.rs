//! Hardware-free logic shared by `ull-esp-platform`.
//!
//! Nothing here touches flash, sockets or peripherals, so the whole crate
//! builds and tests on the host:
//!
//! ```text
//! cargo +stable test -p ull-esp-core --target x86_64-unknown-linux-gnu
//! ```

#![cfg_attr(not(test), no_std)]

pub mod hex;
pub mod http;
pub mod image;
pub mod otadata;
pub mod signature;
