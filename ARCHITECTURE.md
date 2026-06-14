## Architecture

This repository exists to own the ESP32 stack we actually use and to stop
rewriting the same bring-up code in every project.

The architecture should optimize for:

- explicit ownership of the stack
- logical code placement
- low duplication across ESP32 projects
- raw escape hatches when the abstraction is not enough

## Layers

The crate layout should follow this dependency direction:

```text
drivers -> generic only
platform -> ESP mechanics
board -> BSP on top of platform
app -> product policy and behavior
```

Allowed dependencies:

```text
drivers:  depends on embedded-hal traits and generic support crates only
platform: depends on drivers, esp-hal, embassy, esp-radio, esp-rtos, etc.
board:    may depend on platform and drivers
app:      may depend on board, platform, and drivers
```

Forbidden dependencies:

- `platform` must not depend on `board`
- `drivers` must not depend on `platform`, `board`, or app code
- `board` must not contain product policy

## Layer Responsibilities

### Drivers

Reusable chip and module drivers.

- own protocol-level behavior
- use `embedded-hal` or `embedded-hal-async`
- no ESP-specific code
- no board-specific assumptions

Examples:

- `ull-sht3x`
- `ull-ssd1306`

### Platform

Reusable ESP32 stack mechanics.

- runtime startup
- Wi-Fi bring-up
- network stack setup
- shared bus helpers
- flash and OTA mechanics
- reusable storage and peripheral helpers

This layer is board-agnostic. It should know how the ESP stack works, not what
physical board is attached.

`ull-esp-platform` is the crate that should own this role.

### Board

Board support package for a physical board.

- pin mapping
- onboard peripherals
- LED polarity and board-specific semantics
- default peripheral assignments
- board-specific convenience composition on top of `platform`

This crate should be allowed to depend on `platform`.

That is the main architectural decision: the board crate is not just a pile of
HAL pin aliases. It is a BSP for the stack we actually use.

Board crates should still expose raw escape hatches when needed. Convenience is
good, but it must not trap the app in a closed abstraction.

### App

Product logic and policy.

- task topology
- endpoint selection
- retry policy
- credentials and config source
- telemetry format
- OTA policy
- product-specific workflows

The app composes the board and platform layers, but it should not need to own
generic ESP bring-up code.

## Code Placement Rule

When deciding where code belongs, ask one question:

> Is this true because of the ESP stack, because of the physical board, or
> because of this product?

- ESP stack truth -> `platform`
- board truth -> `board`
- product truth -> `app`

## Design Rules

- prefer explicit resource structs over framework-style global managers
- keep defaults, but do not hardcode policy as the only path
- expose raw parts when practical
- keep app behavior and retry logic out of the lower layers
- split crates by dependency boundaries, not by aesthetics

## Implications For This Repo

1. `ull-esp-platform` is the ESP stack crate for this repo
2. `ull-esp-board-devkit-v1` should become a real BSP crate
3. board crates may compose platform defaults, while still exposing raw parts
4. examples should keep product policy and task topology
5. reusable ESP mechanics should move out of examples and into `platform`

## Non-Goals

- a giant framework crate that hides the hardware model
- fake ownership created by wrapping every third-party type
- a strict HAL-only board layer that forces repeated app boilerplate
