#![no_std]

extern crate alloc;

pub use config::WifiConfig;
pub use error::EspError;
pub use i2c::{I2cConfig, SharedI2c, SharedI2cBus, SharedI2cResources};
pub use wifi::{StationNetworkConfig, WifiRunner, WifiStackParts, WifiStackResources};

pub mod config;
pub mod error;
pub mod i2c;
pub mod runtime;
pub mod wifi;
