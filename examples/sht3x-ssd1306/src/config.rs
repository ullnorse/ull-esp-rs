use core::net::Ipv4Addr;

use embassy_net::Ipv4Address;

use crate::error::AppError;

pub const WIFI_SSID: &str = env!("WIFI_SSID");
pub const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
pub const READINGS_HOST: &str = env!("READINGS_HOST");

#[derive(Copy, Clone)]
pub struct ReadingsConfig {
    pub host: &'static str,
    pub path: &'static str,
    pub address: Ipv4Address,
    pub port: u16,
}

pub fn wifi_config() -> ull_esp_support::config::WifiConfig<'static> {
    ull_esp_support::config::WifiConfig::new(WIFI_SSID, WIFI_PASSWORD)
}

pub fn readings_config() -> Result<ReadingsConfig, AppError> {
    let (address, port) = readings_server()?;

    Ok(ReadingsConfig {
        host: READINGS_HOST,
        path: readings_path(),
        address,
        port,
    })
}

fn readings_path() -> &'static str {
    match option_env!("READINGS_PATH") {
        Some(path) if !path.is_empty() => path,
        _ => "/",
    }
}

fn readings_port() -> Result<u16, AppError> {
    option_env!("READINGS_PORT")
        .unwrap_or("3000")
        .parse()
        .map_err(|_| AppError::InvalidReadingsPort)
}

fn readings_server() -> Result<(Ipv4Address, u16), AppError> {
    Ok((
        parse_ipv4(READINGS_HOST).ok_or(AppError::InvalidReadingsHost)?,
        readings_port()?,
    ))
}

fn parse_ipv4(value: &str) -> Option<Ipv4Address> {
    let parsed: Ipv4Addr = value.parse().ok()?;
    let [a, b, c, d] = parsed.octets();
    Some(Ipv4Address::new(a, b, c, d))
}
