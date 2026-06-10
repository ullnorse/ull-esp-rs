# ull-esp-rs

Personal ESP32 support workspace for `esp-hal` + `esp-rtos` + `embassy` projects.

For the current design direction and migration notes, see [`CONTEXT.md`](./CONTEXT.md).

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
│   ├── ull-esp-support/
│   └── ull-esp-board-devkit-v1/
└── examples/
    └── sht3x-ssd1306/
```

## Crates

 - `ull-esp-support`: reusable runtime, I2C, and Wi-Fi setup for ESP32 projects using Embassy.
 - `ull-esp-board-devkit-v1`: HAL-only board mapping and raw parts for an ESP32 DevKit V1 style board.

## API Direction

The intended shape is explicit parts over framework-style wrappers. Board crates stay HAL-only and apps compose them with `ull-esp-support`:

```rust
let ull_esp_board_devkit_v1::Board {
    runtime,
    wifi,
    i2c0,
    pins: _pins,
} = ull_esp_board_devkit_v1::Board::init(ull_esp_support::runtime::max_clock_config());

ull_esp_support::runtime::start(runtime.timg0, runtime.sw_interrupt);

let i2c = ull_esp_support::i2c::init_i2c(i2c0.controller, i2c0.pins.scl, i2c0.pins.sda)?;
let wifi = ull_esp_support::wifi::init_station_dhcp(wifi, seed, &WIFI_STACK_RESOURCES)?;
```

The board crate exposes board-specific raw parts. The support crate turns those parts into reusable runtime services.

Common support types are also re-exported from `ull_esp_support` so app code can usually import from the crate root instead of reaching into each module.

## Examples

- `examples/sht3x-ssd1306`: end-to-end example using Wi-Fi, shared I2C, SHT3x, and SSD1306.

Each example that needs compile-time configuration should keep its own `.env.example`
and local `.env`, loaded by that example's `build.rs`.

At the moment the example uses sibling path dependencies to the local `ull-drivers-rs` checkout.

## Commands

```bash
cargo check --workspace
```
