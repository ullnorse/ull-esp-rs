# ull-esp-rs

Personal ESP32 support workspace for `esp-hal` + `esp-rtos` + `embassy` projects.

For the current design direction, see [`ARCHITECTURE.md`](./ARCHITECTURE.md).

This repository sits between app code and device drivers:

- `ull-drivers-rs` owns reusable chip and module drivers
- `ull-esp-rs` owns ESP32 runtime, Wi-Fi, board, flash, and OTA setup
- app repositories own task topology and product logic

## Goals

- make new ESP32 projects faster to start
- keep an Arduino-like happy path without hiding raw control
- standardize the repetitive runtime and Wi-Fi setup
- keep board-specific code separate from generic ESP32 support
- avoid framework-style abstractions that fight the hardware model

## Workspace Layout

```text
ull-esp-rs/
├── crates/
│   ├── ull-esp-core/
│   └── ull-esp-platform/
├── boards/
│   └── esp32-devkit-v1/
└── examples/
    ├── blinky/
    ├── http-ota/
    ├── ota-layout/
    └── sht3x-ssd1306/
```

`boards/esp32-devkit-v1` contains the Cargo package `ull-esp-board-devkit-v1`.

## Workspace Packages

- `ull-esp-core`: hardware-free logic (otadata rules, image header and app descriptor checks, OTA signature verification, HTTP response parsing, the fleet check-in protocol, restart notes). Builds and tests on the host.
- `ull-esp-platform`: reusable runtime, Wi-Fi, flash, OTA, fleet agent, restart reasons, watchdog, and peripheral helpers for ESP32 projects using Embassy.
- `ull-esp-board-devkit-v1`: ESP32 DevKit V1 board mapping and convenience helpers built on `ull-esp-platform`.
- `ull-esp-board-pocket`: ull-pocket (ESP32-C6) pin map, buttons, user LED, charger control, e-paper bus and the board's firmware rules. Not run on hardware yet.

## Chips

`ull-esp-platform` builds for the ESP32 (`esp32` feature) and the ESP32-C6
(`esp32c6`); each board crate turns on its chip. Build one board's crates at
a time. ESP32 crates use the default target; ESP32-C6 crates go through the
aliases in `.cargo/config.toml`, which add `.cargo/esp32c6.toml` (RISC-V
target, chip revision setting):

```bash
cargo build --release -p sht3x-ssd1306      # ESP32
cargo c6-build --release -p pocket-hello    # ESP32-C6
```

The OTA bootloader in `examples/ota-layout` is built for the ESP32 only.

## API Direction

The intended shape is explicit parts over framework-style wrappers:

- `ull-esp-platform` owns generic ESP bring-up under module-qualified APIs like `runtime`, `wifi`, `flash`, `ota`, `i2c`, `spi`, `uart`, `config`, and `error`.
- `ull-esp-board-devkit-v1` owns DevKit V1 pin mapping and board-specific convenience composition on top of `ull-esp-platform`.

Typical app code can stay on the board convenience path. `Board::init()` keeps
only the peripherals the board maps; use `take_board!` when the app needs others:

```rust
let peripherals = esp_hal::init(Board::default_config());
let mut board = ull_esp_board_devkit_v1::take_board!(peripherals);
let adc = peripherals.ADC1; // not mapped by the board, still usable
ull_esp_platform::runtime::init_default_heap();
board.start_runtime()?; // `Board::init()` does both itself

let i2c_bus = board.take_i2c0_shared()?;
let spi = board.take_spi2()?;
let spi_cs = board.take_spi2_cs()?;
let uart = board.take_uart2()?;

let wifi_config = ull_esp_platform::config::WifiConfig::new("ssid", "password");
let wifi = board.take_wifi_station(spawner, &wifi_config)?;
```

Board defaults: I2C0 runs at 400 kHz (`take_i2c0_with_config` for 100 kHz parts),
and `take_wifi_station` reserves 3 sockets (`take_wifi_station_with` takes your
own `WifiStackResources<N>`).

Crash recovery: linking `ull-esp-platform` installs a panic handler that logs,
keeps the panic message in RTC memory and resets the device.
`board.start_watchdog(spawner, &PULSES)` reboots when a watched task stops
reporting progress, and the hardware watchdog resets the chip when no task runs.
`ull_esp_platform::restart::last()` tells the next boot why it happened; the
fleet agent reports it.

Settings: `ull_esp_platform::settings::load` reads the Wi-Fi name, password and
fleet server URL saved in the `nvs` partition, falling back to the ones built
into the firmware. With none, or after the BOOT button is held for 3 seconds
(`board.watch_setup_button`), or after 5 minutes without Wi-Fi
(`board.watch_wifi_for_setup`), the board restarts into setup mode
(`board.run_setup`): it opens the open network `ull-setup-xxxx` (the last four
hex digits of its MAC), and a phone that joins it is shown a page to enter the
settings. Saving restarts the board with them; setup mode also restarts after
10 minutes without a request.

Lower-level platform helpers remain available under module-qualified paths such as `ull_esp_platform::i2c::init_i2c`, `ull_esp_platform::spi::init_spi`, and `ull_esp_platform::uart::init_uart`.

## Examples

- `examples/blinky`: smallest board smoke test for runtime startup and the status LED.
- `examples/pocket-hello`: first power-up test for ull-pocket (ESP32-C6): buttons, user LED, USB power and wake lines.
- `examples/sht3x-ssd1306`: fleet node: SHT3x readings on an SSD1306 and reported to the ull-fleet server, with signed OTA updates.
- `examples/http-ota`: the fleet agent alone (heartbeats and OTA), no sensors.
- `examples/ota-layout`: bootloader and partition table for the OTA examples, first-flash and update-image commands, install checks.

## Fleet agent

`ull_esp_platform::fleet::Agent` talks to
[ull-fleet-rs](https://github.com/ullnorse/ull-fleet-rs): it checks in every
30 s with firmware identity, Wi-Fi signal and the latest metrics, installs an
update when the server offers one, and confirms or rolls back the new image.

```rust
ull_esp_platform::firmware_app_desc!(); // version and secure_version per build

static METRICS: FleetMetrics = FleetMetrics::new();

fleet::start(spawner, stack, flash, settings.server_url.as_str(), &ESP_APP_DESC, &METRICS)?;
METRICS.set("temperature_c", 21.5);
```

The update key is `OTA_PUBLIC_KEY` from the shared configuration below. For
another key or your own task, build a `fleet::Agent` and call `run` on it.

`firmware_app_desc!` needs `export_firmware_identity()` from
`examples/build_support.rs` in the crate's `build.rs`.

## Shared configuration

The Wi-Fi network, fleet server and update key live in one file for every
project: `~/.config/ull-esp/device.env` (template: `device.env.example`; another
path with `ULL_ESP_CONFIG`). `ull-esp-platform`'s `build.rs` bakes them in, so a
change needs a rebuild; a variable set in the environment wins for one build.
`settings::built_in()` returns the Wi-Fi and server values, and settings saved
on a board win over them.

## A new app

`main.rs` is the same in every app; the app's own code goes in `app::run`:

```rust
#![no_std]
#![no_main]

use embassy_executor::Spawner;

mod app;

ull_esp_platform::firmware_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    ull_esp_platform::runtime::run_app(spawner, app::run).await
}
```

`run_app` starts logging and, when `app::run` returns an error, logs it and
resets the device. See `examples/sht3x-ssd1306/src/app.rs` for an app with
settings, setup mode, Wi-Fi, updates and sensors.

`examples/sht3x-ssd1306` also depends on a sibling `../ull-drivers-rs` checkout
for `ull-sht3x` and `ull-ssd1306`.

## Prerequisites

- the checked-in Rust toolchain channel is `esp`
- the checked-in Cargo config builds for `xtensa-esp32-none-elf` by default and enables `build-std = ["alloc", "core"]`
- `examples/sht3x-ssd1306` needs the sibling `ull-drivers-rs` repository
- `examples/http-ota` and `examples/sht3x-ssd1306` read `~/.config/ull-esp/device.env` (see Shared configuration)

## Commands

```bash
# self-contained smoke test
cargo check -p ull-esp-platform -p ull-esp-board-devkit-v1 -p blinky -p http-ota

# full workspace check; requires sibling ull-drivers-rs and example env setup
cargo check --workspace

# host unit tests for the hardware-free logic (otadata, signatures, HTTP)
cargo +stable test -p ull-esp-core --target x86_64-unknown-linux-gnu
```

The Xtensa-only rustflags live under `[target.xtensa-esp32-none-elf]` in
`.cargo/config.toml`, and stable Cargo ignores the `[unstable]` build-std
table, so host builds of `ull-esp-core` work from the workspace root.
