use embassy_executor::Spawner;
use embassy_net::Stack;
use ull_esp_platform::config::WifiConfig;
use ull_esp_platform::wifi::{StationNetworkConfig, WifiStackResources};

use super::{Board, BoardError};

/// Sockets reserved by `take_wifi_station`: DHCP plus two TCP connections.
/// Apps that need more pass their own `WifiStackResources` to
/// `take_wifi_station_with`.
pub const DEFAULT_WIFI_SOCKETS: usize = 3;

static DEFAULT_WIFI_STACK_RESOURCES: WifiStackResources<DEFAULT_WIFI_SOCKETS> =
    WifiStackResources::new();

pub struct WifiStation {
    stack: Stack<'static>,
}

pub(super) struct WifiParts {
    pub(super) peripheral: esp_hal::peripherals::WIFI<'static>,
}

impl WifiStation {
    pub fn stack(&self) -> Stack<'static> {
        self.stack
    }
}

impl Board {
    /// Wi-Fi station with DHCP and [`DEFAULT_WIFI_SOCKETS`] sockets.
    pub fn take_wifi_station(
        &mut self,
        spawner: Spawner,
        config: &WifiConfig<'_>,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with(
            spawner,
            config,
            StationNetworkConfig::default(),
            &DEFAULT_WIFI_STACK_RESOURCES,
        )
    }

    /// Wi-Fi station with an explicit network config and socket count:
    ///
    /// ```ignore
    /// static RESOURCES: WifiStackResources<6> = WifiStackResources::new();
    /// board.take_wifi_station_with(spawner, &config, StationNetworkConfig::dhcpv4(), &RESOURCES)?;
    /// ```
    pub fn take_wifi_station_with<const SOCKETS: usize>(
        &mut self,
        spawner: Spawner,
        config: &WifiConfig<'_>,
        net_config: StationNetworkConfig,
        resources: &'static WifiStackResources<SOCKETS>,
    ) -> Result<WifiStation, BoardError> {
        let parts = self.wifi.take().ok_or(BoardError::AlreadyTaken("wifi"))?;
        let mut wifi = ull_esp_platform::wifi::init_station(
            parts.peripheral,
            Self::wifi_seed(),
            resources,
            net_config,
        )?;
        ull_esp_platform::wifi::configure(&mut wifi.controller, config)?;

        let connection = ull_esp_platform::wifi::connection_task(wifi.controller)
            .map_err(|_| BoardError::TaskSpawn("wifi"))?;
        spawner.spawn(connection);

        let runner = ull_esp_platform::wifi::runner_task(wifi.runner)
            .map_err(|_| BoardError::TaskSpawn("net"))?;
        spawner.spawn(runner);

        Ok(WifiStation { stack: wifi.stack })
    }
}
