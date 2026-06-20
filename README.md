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
│   └── ull-esp-platform/
├── boards/
│   └── esp32-devkit-v1/
└── examples/
    ├── blinky/
    ├── http-ota/
    └── sht3x-ssd1306/
```

`boards/esp32-devkit-v1` contains the Cargo package `ull-esp-board-devkit-v1`.

## Workspace Packages

- `ull-esp-platform`: reusable runtime, Wi-Fi, flash, OTA, and peripheral helpers for ESP32 projects using Embassy.
- `ull-esp-board-devkit-v1`: ESP32 DevKit V1 board mapping and convenience helpers built on `ull-esp-platform`.

## API Direction

The intended shape is explicit parts over framework-style wrappers:

- `ull-esp-platform` owns generic ESP bring-up under module-qualified APIs like `runtime`, `wifi`, `flash`, `ota`, `i2c`, `spi`, `uart`, `config`, and `error`.
- `ull-esp-board-devkit-v1` owns DevKit V1 pin mapping and board-specific convenience composition on top of `ull-esp-platform`.

Typical app code can stay on the board convenience path:

```rust
let mut board = ull_esp_board_devkit_v1::Board::init();
board.start_runtime()?;

let i2c_bus = board.take_i2c0_shared()?;
let spi = board.take_spi2()?;
let spi_cs = board.take_spi2_cs()?;
let uart = board.take_uart2()?;

let wifi_config = ull_esp_platform::config::WifiConfig::new("ssid", "password");
let wifi = board.take_wifi_station(spawner, &wifi_config)?;
```

Lower-level platform helpers remain available under module-qualified paths such as `ull_esp_platform::i2c::init_i2c`, `ull_esp_platform::spi::init_spi`, and `ull_esp_platform::uart::init_uart`.

## Examples

- `examples/blinky`: smallest board smoke test for runtime startup and the status LED.
- `examples/sht3x-ssd1306`: end-to-end example using Wi-Fi, shared I2C, SHT3x, and SSD1306.
- `examples/http-ota`: Device-Pull OTA example with example-scoped fleet polling on top of `ull-esp-platform::ota`.

Examples that need compile-time configuration keep their own `.env.example`
and local `.env`, loaded by that example's `build.rs`. Those values are baked
into the firmware at build time, so changing them requires a rebuild.

`examples/sht3x-ssd1306` also depends on a sibling `../ull-drivers-rs` checkout
for `ull-sht3x` and `ull-ssd1306`.

## Prerequisites

- the checked-in Rust toolchain channel is `esp`
- the checked-in Cargo config builds for `xtensa-esp32-none-elf` by default and enables `build-std = ["alloc", "core"]`
- `examples/sht3x-ssd1306` needs the sibling `ull-drivers-rs` repository
- `examples/http-ota` and `examples/sht3x-ssd1306` expect build-time environment variables from `.env` or the shell

## Commands

```bash
# self-contained smoke test
cargo check -p ull-esp-platform -p ull-esp-board-devkit-v1 -p blinky -p http-ota

# full workspace check; requires sibling ull-drivers-rs and example env setup
cargo check --workspace
```
