use embassy_net::{Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::{Interface as WifiDevice, WifiController};
use static_cell::StaticCell;

use crate::config::{WifiConfig, station_wifi_configuration};
use crate::error::EspError;

pub type WifiRunner = embassy_net::Runner<'static, WifiDevice<'static>>;

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

pub fn init_station_dhcp<const SOCKETS: usize>(
    wifi: WIFI<'static>,
    seed: u64,
    resources: &'static WifiStackResources<SOCKETS>,
) -> Result<WifiStackParts, EspError> {
    let (controller, interfaces) = esp_radio::wifi::new(wifi, Default::default())?;
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        resources.stack.init(StackResources::new()),
        seed,
    );

    Ok(WifiStackParts {
        controller,
        stack,
        runner,
    })
}

pub fn configure(
    controller: &mut WifiController<'static>,
    config: &WifiConfig<'_>,
) -> Result<(), EspError> {
    let config = station_wifi_configuration(config);
    controller.set_config(&config)?;
    Ok(())
}

#[embassy_executor::task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(_) => {
                log::info!("wifi connected");
                if let Err(err) = controller.wait_for_disconnect_async().await {
                    log::warn!("wifi disconnect wait failed: {err:?}");
                    Timer::after(Duration::from_secs(2)).await;
                } else {
                    log::warn!("wifi disconnected");
                }
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
