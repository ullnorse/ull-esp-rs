use core::net::Ipv4Addr;

use embassy_net::Ipv4Address;
use ull_esp_platform::config::WifiConfig;

use crate::error::AppError;

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
    pub endpoint: FleetEndpoint,
}

impl FleetConfig {
    pub fn from_env() -> Result<Self, AppError> {
        Ok(Self {
            endpoint: FleetEndpoint::from_base_url(
                option_env!("FLEET_BASE_URL").ok_or(AppError::MissingFleetBaseUrl)?,
            )?,
        })
    }
}

#[derive(Copy, Clone)]
pub struct FleetEndpoint {
    pub authority: &'static str,
    pub server_ip: Ipv4Address,
    pub port: u16,
}

impl FleetEndpoint {
    fn from_base_url(base_url: &'static str) -> Result<Self, AppError> {
        let authority = base_url
            .strip_prefix("http://")
            .ok_or(AppError::InvalidFleetBaseUrl)?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);

        if authority.is_empty() || authority.contains('/') {
            return Err(AppError::InvalidFleetBaseUrl);
        }

        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host,
                port.parse().map_err(|_| AppError::InvalidFleetBaseUrl)?,
            ),
            None => (authority, 80),
        };

        Ok(Self {
            authority,
            server_ip: parse_ipv4(host).ok_or(AppError::InvalidFleetBaseUrl)?,
            port,
        })
    }
}

fn parse_ipv4(value: &str) -> Option<Ipv4Address> {
    let parsed: Ipv4Addr = value.parse().ok()?;
    let [a, b, c, d] = parsed.octets();
    Some(Ipv4Address::new(a, b, c, d))
}
