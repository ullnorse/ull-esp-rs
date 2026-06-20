# ull-esp-rs

Personal ESP32 support workspace for `esp-hal` + `esp-rtos` + `embassy` projects.

For the current design direction, see [`ARCHITECTURE.md`](./ARCHITECTURE.md).

This repository sits between app code and device drivers:

- `ull-drivers-rs` owns reusable chip and module drivers
- `ull-esp-rs` owns ESP32 runtime, Wi-Fi, board, and peripheral setup
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
│   ├── ull-esp-platform/
│   └── ull-esp-board-devkit-v1/
└── examples/
    ├── blinky/
    ├── http-ota/
    └── sht3x-ssd1306/
```

## Crates

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

- `examples/sht3x-ssd1306`: end-to-end example using Wi-Fi, shared I2C, SHT3x, and SSD1306.
- `examples/http-ota`: Device-Pull OTA example with example-scoped fleet polling on top of `ull-esp-platform::ota`.

Each example that needs compile-time configuration should keep its own `.env.example`
and local `.env`, loaded by that example's `build.rs`.

At the moment the example uses sibling path dependencies to the local `ull-drivers-rs` checkout.

## Commands

```bash
cargo check --workspace
```
