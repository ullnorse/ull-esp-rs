use embassy_executor::Spawner;
use embassy_net::Stack;
use esp_hal::gpio::{Input, InputConfig, Pull};
use ull_esp_platform::flash::FlashStorageDevice;
use ull_esp_platform::settings::Settings;
use ull_esp_platform::setup;

use super::{Board, BoardError};

impl Board {
    /// Runs setup mode (see `ull_esp_platform::setup`) until it reboots:
    /// the board opens the `ull-setup-xxxx` network with a settings page.
    pub async fn run_setup(
        &mut self,
        spawner: Spawner,
        flash: FlashStorageDevice,
        current: &'static Settings,
        firmware: &'static str,
    ) -> Result<core::convert::Infallible, BoardError> {
        let parts = self.wifi.take().ok_or(BoardError::AlreadyTaken("wifi"))?;
        setup::run(
            spawner,
            parts.peripheral,
            flash,
            current,
            firmware,
            ull_esp_platform::wifi::random_seed(),
        )
        .await
    }

    /// Holding the BOOT button for 3 seconds reboots into setup mode.
    pub fn watch_setup_button(&mut self, spawner: Spawner) -> Result<(), BoardError> {
        let pin = self
            .pins
            .boot_button
            .take()
            .ok_or(BoardError::AlreadyTaken("boot button"))?;
        let button = Input::new(pin, InputConfig::default().with_pull(Pull::Up));
        spawner
            .spawn(setup::button_task(button).map_err(|_| BoardError::TaskSpawn("setup button"))?);
        Ok(())
    }

    /// Reboots into setup mode after 5 minutes without Wi-Fi.
    pub fn watch_wifi_for_setup(
        &self,
        spawner: Spawner,
        stack: Stack<'static>,
    ) -> Result<(), BoardError> {
        spawner.spawn(
            setup::fallback_task(stack).map_err(|_| BoardError::TaskSpawn("setup fallback"))?,
        );
        Ok(())
    }
}
