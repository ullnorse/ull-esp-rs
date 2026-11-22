use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfiguration, Password, Ssid, WifiError,
    sta::StationConfig,
};

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

/// Builds the station config. esp-radio rejects an SSID over 32 bytes or an
/// oversized password instead of truncating it.
pub(crate) fn station_wifi_configuration(
    config: &WifiConfig<'_>,
) -> Result<WifiConfiguration, WifiError> {
    let authentication = if config.password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(Password::try_from(config.password)?)
    };

    Ok(WifiConfiguration::Station(
        StationConfig::default()
            .with_ssid(Ssid::try_from(config.ssid)?)
            .with_authentication(authentication),
    ))
}
