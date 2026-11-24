use esp_hal::peripherals::FLASH as Flash;
use esp_storage::FlashStorage;

pub type FlashStorageDevice = FlashStorage<'static>;

/// Flash storage that parks the app core during writes and erases.
///
/// The ESP32 is dual-core. Without a strategy, esp-storage refuses to write
/// (`OtherCoreRunning`) while the second core runs, because that core could be
/// executing from flash. Auto-park stops it for the duration of the operation.
#[cfg(feature = "esp32")]
pub fn init_flash_storage(flash: Flash<'static>) -> FlashStorageDevice {
    FlashStorage::new(flash).multicore_auto_park()
}

/// Flash storage. Single-core chips need no core parking.
#[cfg(not(feature = "esp32"))]
pub fn init_flash_storage(flash: Flash<'static>) -> FlashStorageDevice {
    FlashStorage::new(flash)
}
