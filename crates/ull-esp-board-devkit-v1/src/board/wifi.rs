use embassy_executor::Spawner;
use embassy_net::Stack;

use super::{Board, BoardError};

static WIFI_STACK_RESOURCES: ull_esp_platform::WifiStackResources<3> =
    ull_esp_platform::WifiStackResources::new();

pub struct WifiStation {
    stack: Stack<'static>,
}

pub struct WifiParts {
    pub peripheral: esp_hal::peripherals::WIFI<'static>,
}

impl WifiParts {
    pub fn into_station<const SOCKETS: usize>(
        self,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<ull_esp_platform::WifiStackParts, ull_esp_platform::EspError> {
        ull_esp_platform::wifi::init_station(self.peripheral, seed, resources, config)
    }

    pub fn into_station_dhcp<const SOCKETS: usize>(
        self,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
    ) -> Result<ull_esp_platform::WifiStackParts, ull_esp_platform::EspError> {
        self.into_station(
            seed,
            resources,
            ull_esp_platform::StationNetworkConfig::default(),
        )
    }
}

impl WifiStation {
    pub fn stack(&self) -> Stack<'static> {
        self.stack
    }
}

impl Board {
    pub fn take_wifi_parts(&mut self) -> Result<WifiParts, BoardError> {
        self.wifi.take().ok_or(BoardError::AlreadyTaken("wifi"))
    }

    pub fn take_wifi_station_dhcp(
        &mut self,
        spawner: Spawner,
        config: &ull_esp_platform::WifiConfig<'_>,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_dhcp_with_seed(spawner, Self::wifi_seed(), config)
    }

    pub fn take_wifi_station_dhcp_with_seed(
        &mut self,
        spawner: Spawner,
        seed: u64,
        config: &ull_esp_platform::WifiConfig<'_>,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with_seed(
            spawner,
            seed,
            config,
            ull_esp_platform::StationNetworkConfig::default(),
        )
    }

    pub fn take_wifi_station(
        &mut self,
        spawner: Spawner,
        config: &ull_esp_platform::WifiConfig<'_>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with_seed(spawner, Self::wifi_seed(), config, net_config)
    }

    pub fn take_wifi_station_with_seed(
        &mut self,
        spawner: Spawner,
        seed: u64,
        config: &ull_esp_platform::WifiConfig<'_>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with_resources_and_seed(
            spawner,
            seed,
            &WIFI_STACK_RESOURCES,
            config,
            net_config,
        )
    }

    pub fn take_wifi_station_dhcp_with_resources<const SOCKETS: usize>(
        &mut self,
        spawner: Spawner,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        config: &ull_esp_platform::WifiConfig<'_>,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_dhcp_with_resources_and_seed(
            spawner,
            Self::wifi_seed(),
            resources,
            config,
        )
    }

    pub fn take_wifi_station_dhcp_with_resources_and_seed<const SOCKETS: usize>(
        &mut self,
        spawner: Spawner,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        config: &ull_esp_platform::WifiConfig<'_>,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with_resources_and_seed(
            spawner,
            seed,
            resources,
            config,
            ull_esp_platform::StationNetworkConfig::default(),
        )
    }

    pub fn take_wifi_station_with_resources<const SOCKETS: usize>(
        &mut self,
        spawner: Spawner,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        config: &ull_esp_platform::WifiConfig<'_>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<WifiStation, BoardError> {
        self.take_wifi_station_with_resources_and_seed(
            spawner,
            Self::wifi_seed(),
            resources,
            config,
            net_config,
        )
    }

    pub fn take_wifi_station_with_resources_and_seed<const SOCKETS: usize>(
        &mut self,
        spawner: Spawner,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        config: &ull_esp_platform::WifiConfig<'_>,
        net_config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<WifiStation, BoardError> {
        let mut wifi = self
            .take_wifi_parts()?
            .into_station(seed, resources, net_config)?;
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
