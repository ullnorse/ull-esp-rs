use alloc::string::String as AllocString;

use esp_radio::wifi::{AuthenticationMethod, Config as WifiConfiguration, sta::StationConfig};

#[derive(Copy, Clone, Debug)]
pub struct WifiConfig<'a> {
    pub ssid: &'a str,
    pub password: &'a str,
}

impl<'a> WifiConfig<'a> {
    pub const fn new(ssid: &'a str, password: &'a str) -> Self {
        Self { ssid, password }
    }
}

pub(crate) fn station_wifi_configuration(config: &WifiConfig<'_>) -> WifiConfiguration {
    WifiConfiguration::Station(
        StationConfig::default()
            .with_ssid(config.ssid)
            .with_password(AllocString::from(config.password))
            .with_auth_method(if config.password.is_empty() {
                AuthenticationMethod::None
            } else {
                AuthenticationMethod::Wpa2Personal
            }),
    )
}
