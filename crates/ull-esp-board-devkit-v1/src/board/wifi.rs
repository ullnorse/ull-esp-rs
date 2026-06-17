use embassy_executor::Spawner;
use embassy_net::Stack;

use super::{Board, BoardError};

static WIFI_STACK_RESOURCES: ull_esp_platform::WifiStackResources<3> =
    ull_esp_platform::WifiStackResources::new();

pub struct WifiStation {
    stack: Stack<'static>,
}

pub(super) struct WifiParts {
    pub(super) peripheral: esp_hal::peripherals::WIFI<'static>,
}

impl WifiParts {
    fn into_station<const SOCKETS: usize>(
        self,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<ull_esp_platform::WifiStackParts, ull_esp_platform::EspError> {
        ull_esp_platform::wifi::init_station(self.peripheral, seed, resources, net_config)
    }
}

impl WifiStation {
    pub fn stack(&self) -> Stack<'static> {
        self.stack
    }
}

impl Board {
    pub fn take_wifi_station(
        &mut self,
        spawner: Spawner,
        config: &ull_esp_platform::WifiConfig<'_>,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with_network(
            spawner,
            config,
            ull_esp_platform::StationNetworkConfig::default(),
        )
    }

    pub fn take_wifi_station_with_network(
        &mut self,
        spawner: Spawner,
        config: &ull_esp_platform::WifiConfig<'_>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<WifiStation, BoardError> {
        let mut wifi =
            self.take_wifi_stack_parts(Self::wifi_seed(), &WIFI_STACK_RESOURCES, net_config)?;
        ull_esp_platform::wifi::configure(&mut wifi.controller, config)?;

        let connection = ull_esp_platform::wifi::connection_task(wifi.controller)
            .map_err(|_| BoardError::TaskSpawn("wifi"))?;
        spawner.spawn(connection);

        let runner = ull_esp_platform::wifi::runner_task(wifi.runner)
            .map_err(|_| BoardError::TaskSpawn("net"))?;
        spawner.spawn(runner);

        Ok(WifiStation { stack: wifi.stack })
    }

    fn take_wifi_stack_parts<const SOCKETS: usize>(
        &mut self,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<ull_esp_platform::WifiStackParts, BoardError> {
        let parts = self.wifi.take().ok_or(BoardError::AlreadyTaken("wifi"))?;
        Ok(parts.into_station(seed, resources, net_config)?)
    }
}
