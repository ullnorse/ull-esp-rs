use embassy_net::Ipv4Address;
use ull_esp_platform::config::WifiConfig;
use ull_esp_platform::hex;
use ull_esp_platform::http::BaseUrl;
use ull_esp_platform::ota::PUBLIC_KEY_LEN;

use crate::error::AppError;

/// Ed25519 key that every update must be signed with. Set `OTA_PUBLIC_KEY` (64
/// hex digits, printed by `ull-fleet-sign keygen`) in `.env`; the build fails
/// without it, and a malformed key fails at compile time too.
pub const OTA_PUBLIC_KEY: [u8; PUBLIC_KEY_LEN] = hex::decode_const(env!(
    "OTA_PUBLIC_KEY",
    "set OTA_PUBLIC_KEY in examples/http-ota/.env (see .env.example)"
));

pub struct AppConfig {
    pub wifi: WifiConfig<'static>,
    pub fleet: FleetConfig,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        Ok(Self {
            wifi: WifiConfig::new(
                option_env!("WIFI_SSID").ok_or(AppError::MissingWifiSsid)?,
                option_env!("WIFI_PASSWORD").ok_or(AppError::MissingWifiPassword)?,
            ),
            fleet: FleetConfig::from_env()?,
        })
    }
}

#[derive(Copy, Clone)]
pub struct FleetConfig {
    /// `host:port` for the `Host` header.
    pub authority: &'static str,
    pub server_ip: Ipv4Address,
    pub port: u16,
}

impl FleetConfig {
    pub fn from_env() -> Result<Self, AppError> {
        let url = option_env!("FLEET_BASE_URL").ok_or(AppError::MissingFleetBaseUrl)?;
        let url = BaseUrl::parse(url).ok_or(AppError::InvalidFleetBaseUrl)?;
        Ok(Self {
            authority: url.authority,
            server_ip: url.ip,
            port: url.port,
        })
    }
}
