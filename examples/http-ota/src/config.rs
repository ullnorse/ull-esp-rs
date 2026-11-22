use ull_esp_platform::config::WifiConfig;
use ull_esp_platform::fleet::FleetConfig;
use ull_esp_platform::hex;
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
        let url = option_env!("FLEET_BASE_URL").ok_or(AppError::MissingFleetBaseUrl)?;
        Ok(Self {
            wifi: WifiConfig::new(
                option_env!("WIFI_SSID").ok_or(AppError::MissingWifiSsid)?,
                option_env!("WIFI_PASSWORD").ok_or(AppError::MissingWifiPassword)?,
            ),
            fleet: FleetConfig::parse(url).ok_or(AppError::InvalidFleetBaseUrl)?,
        })
    }
}
