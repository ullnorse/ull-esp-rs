//! Device settings in the `nvs` partition. The format and the slot rules are
//! in `ull_esp_core::settings`; this module only reads and writes flash.
//!
//! Settings saved on the board win over the ones built into the firmware
//! (from `~/.config/ull-esp/device.env`, see [`built_in`]), so a board set up from the phone keeps its
//! network across updates.

use esp_bootloader_esp_idf::partitions::{
    self, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType, read_partition_table,
};
use esp_storage::FlashStorage;
use static_cell::StaticCell;
use thiserror::Error;
use ull_esp_core::settings::{self as core_settings, RECORD_MAX, SLOT_OFFSETS, Stored};

pub use ull_esp_core::settings::{Invalid, Settings};

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("the partition table has no nvs partition")]
    NoPartition,
    #[error(transparent)]
    Partition(#[from] partitions::Error),
    #[error("the saved settings did not read back")]
    Verify,
}

/// Where the settings in use came from.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Saved on the board, from the setup page or the server.
    Saved,
    /// Built into the firmware.
    BuiltIn,
    /// Nothing anywhere: the board needs setup.
    None,
}

static LOADED: StaticCell<Settings> = StaticCell::new();

/// Settings built into the firmware from `~/.config/ull-esp/device.env`
/// (`WIFI_SSID`, `WIFI_PASSWORD`, `FLEET_BASE_URL`; see `build.rs`). Empty
/// when unset or invalid.
pub fn built_in() -> Settings {
    let ssid = option_env!("WIFI_SSID").unwrap_or("");
    let password = option_env!("WIFI_PASSWORD").unwrap_or("");
    let server = option_env!("FLEET_BASE_URL").unwrap_or("");
    Settings::new(ssid, password, server).unwrap_or_else(|invalid| {
        if !ssid.is_empty() {
            log::warn!("ignoring the built-in settings: {}", invalid.message());
        }
        Settings::default()
    })
}

/// Reads the saved settings, or falls back to `built_in`. Call once; the
/// result lives for the rest of the program so the fleet agent can borrow
/// its server URL.
pub fn load(flash: &mut FlashStorage<'_>, built_in: Settings) -> (&'static Settings, Source) {
    let (settings, source) = match read_current(flash) {
        Ok(Some((_, stored))) => (stored.settings, Source::Saved),
        Ok(None) if built_in.has_wifi() => (built_in, Source::BuiltIn),
        Ok(None) => (built_in, Source::None),
        Err(err) => {
            log::warn!("settings: cannot read saved settings: {err}");
            let source = if built_in.has_wifi() {
                Source::BuiltIn
            } else {
                Source::None
            };
            (built_in, source)
        }
    };
    (LOADED.init(settings), source)
}

/// Saves `settings` into the slot that does not hold the current record, so
/// a power loss mid-write keeps the old settings.
pub fn save(flash: &mut FlashStorage<'_>, settings: &Settings) -> Result<(), SettingsError> {
    let current = read_current(flash)?;
    let (slot, seq) = core_settings::next_write(current);
    let mut record = [0u8; RECORD_MAX];
    core_settings::encode(settings, seq, &mut record);
    with_region(flash, |region| {
        // Writing through the region erases and rewrites the whole sector.
        region.write(SLOT_OFFSETS[slot], &record)?;
        Ok(())
    })?;
    // Read back: a save that does not verify must not reboot into nothing.
    match read_current(flash)? {
        Some((_, stored)) if stored.seq == seq && stored.settings == *settings => Ok(()),
        _ => Err(SettingsError::Verify),
    }
}

fn read_current(flash: &mut FlashStorage<'_>) -> Result<Option<(usize, Stored)>, SettingsError> {
    with_region(flash, |region| {
        let mut read = |offset| -> Result<Option<Stored>, SettingsError> {
            let mut bytes = [0u8; RECORD_MAX];
            region.read(offset, &mut bytes)?;
            Ok(core_settings::decode(&bytes))
        };
        Ok(core_settings::newest([
            read(SLOT_OFFSETS[0])?,
            read(SLOT_OFFSETS[1])?,
        ]))
    })
}

fn with_region<T>(
    flash: &mut FlashStorage<'_>,
    f: impl FnOnce(&mut partitions::FlashRegion<'_, '_>) -> Result<T, SettingsError>,
) -> Result<T, SettingsError> {
    let mut buffer = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(flash, &mut buffer)?;
    let entry = table
        .find_partition(PartitionType::Data(DataPartitionSubType::Nvs))?
        .ok_or(SettingsError::NoPartition)?;
    let mut region = entry.as_flash_region(flash);
    f(&mut region)
}
