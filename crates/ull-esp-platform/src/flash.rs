use esp_hal::peripherals::FLASH as Flash;
use esp_storage::FlashStorage;

pub type FlashStorageDevice = FlashStorage<'static>;

#[allow(unexpected_cfgs)]
pub fn init_flash_storage(flash: Flash<'static>) -> FlashStorageDevice {
    let flash = FlashStorage::new(flash);

    #[cfg(multi_core)]
    let flash = flash.multicore_auto_park();

    flash
}
