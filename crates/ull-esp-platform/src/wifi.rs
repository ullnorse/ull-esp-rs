use core::sync::atomic::{AtomicI32, Ordering};

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_net::{Config as NetConfig, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::{Interface as WifiDevice, WifiController};
use static_cell::StaticCell;

use crate::config::{WifiConfig, station_wifi_configuration};
use crate::error::EspError;

pub type WifiRunner = embassy_net::Runner<'static, WifiDevice>;

pub struct StationNetworkConfig {
    stack: NetConfig,
}

pub struct WifiStackParts {
    pub controller: WifiController<'static>,
    pub stack: Stack<'static>,
    pub runner: WifiRunner,
}

pub struct WifiStackResources<const SOCKETS: usize> {
    stack: StaticCell<StackResources<SOCKETS>>,
}

impl<const SOCKETS: usize> WifiStackResources<SOCKETS> {
    pub const fn new() -> Self {
        Self {
            stack: StaticCell::new(),
        }
    }
}

impl<const SOCKETS: usize> Default for WifiStackResources<SOCKETS> {
    fn default() -> Self {
        Self::new()
    }
}

impl StationNetworkConfig {
    pub fn new(stack: NetConfig) -> Self {
        Self { stack }
    }

    pub fn dhcpv4() -> Self {
        Self::new(NetConfig::dhcpv4(Default::default()))
    }

    pub fn ipv4_static(config: StaticConfigV4) -> Self {
        Self::new(NetConfig::ipv4_static(config))
    }
}

impl Default for StationNetworkConfig {
    fn default() -> Self {
        Self::dhcpv4()
    }
}

pub fn init_station<const SOCKETS: usize>(
    wifi: WIFI<'static>,
    seed: u64,
    resources: &'static WifiStackResources<SOCKETS>,
    config: StationNetworkConfig,
) -> Result<WifiStackParts, EspError> {
    let controller = WifiController::new(wifi, Default::default())?;
    let (stack, runner) = embassy_net::new(
        WifiDevice::station(),
        config.stack,
        resources.stack.init(StackResources::new()),
        seed,
    );

    Ok(WifiStackParts {
        controller,
        stack,
        runner,
    })
}

pub fn init_station_dhcp<const SOCKETS: usize>(
    wifi: WIFI<'static>,
    seed: u64,
    resources: &'static WifiStackResources<SOCKETS>,
) -> Result<WifiStackParts, EspError> {
    init_station(wifi, seed, resources, StationNetworkConfig::default())
}

/// Starts a Wi-Fi station: configures the radio, spawns the connection and
/// network tasks, and returns the stack. It connects in the background and
/// reconnects after every drop.
pub fn start_station<const SOCKETS: usize>(
    spawner: Spawner,
    wifi: WIFI<'static>,
    config: &WifiConfig<'_>,
    net_config: StationNetworkConfig,
    resources: &'static WifiStackResources<SOCKETS>,
) -> Result<Stack<'static>, EspError> {
    let mut parts = init_station(wifi, random_seed(), resources, net_config)?;
    configure(&mut parts.controller, config)?;
    spawner.spawn(connection_task(parts.controller).map_err(|_| EspError::TaskSpawn("wifi"))?);
    spawner.spawn(runner_task(parts.runner).map_err(|_| EspError::TaskSpawn("net"))?);
    Ok(parts.stack)
}

/// A random seed for the network stack, from the hardware RNG.
pub fn random_seed() -> u64 {
    let rng = esp_hal::rng::Rng::new();
    (u64::from(rng.random()) << 32) | u64::from(rng.random())
}

pub fn configure(
    controller: &mut WifiController<'static>,
    config: &WifiConfig<'_>,
) -> Result<(), EspError> {
    let config = station_wifi_configuration(config)?;
    controller.set_config(&config)?;
    Ok(())
}

/// Last RSSI of the station link in dBm; 0 while unknown or disconnected.
static RSSI_DBM: AtomicI32 = AtomicI32::new(0);
const RSSI_SAMPLE_PERIOD: Duration = Duration::from_secs(10);

/// Signal strength of the current Wi-Fi link, sampled by [`connection_task`].
pub fn rssi_dbm() -> Option<i32> {
    match RSSI_DBM.load(Ordering::Relaxed) {
        0 => None,
        rssi => Some(rssi),
    }
}

fn sample_rssi(controller: &WifiController<'static>) {
    RSSI_DBM.store(controller.rssi().unwrap_or(0), Ordering::Relaxed);
}

#[embassy_executor::task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(_) => {
                log::info!("wifi connected");
                sample_rssi(&controller);
                loop {
                    let disconnect = controller.wait_for_disconnect_async();
                    match select(disconnect, Timer::after(RSSI_SAMPLE_PERIOD)).await {
                        Either::First(Ok(_)) => log::warn!("wifi disconnected"),
                        Either::First(Err(err)) => {
                            log::warn!("wifi disconnect wait failed: {err:?}");
                            Timer::after(Duration::from_secs(2)).await;
                        }
                        Either::Second(()) => {
                            sample_rssi(&controller);
                            continue;
                        }
                    }
                    break;
                }
                RSSI_DBM.store(0, Ordering::Relaxed);
            }
            Err(err) => {
                log::warn!("wifi connect failed: {err:?}");
                Timer::after(Duration::from_secs(5)).await;
            }
        }
    }
}

#[embassy_executor::task]
pub async fn runner_task(mut runner: WifiRunner) {
    runner.run().await;
}
