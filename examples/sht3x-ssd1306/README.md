# sht3x-ssd1306

A fleet node: reads an SHT3x temperature and humidity sensor, shows the values
on an SSD1306 display, and reports them to the ull-fleet server as
`temperature_c` and `humidity_pct`. Updates arrive over the air through the
same `ull_esp_platform::fleet` agent as `http-ota`.

Wiring: both parts on I2C0 (SDA GPIO21, SCL GPIO22, 400 kHz), addresses 0x44
and 0x3C.

## Configuration

Copy `.env.example` to `.env` and set `WIFI_SSID`, `WIFI_PASSWORD`,
`FLEET_BASE_URL` and `OTA_PUBLIC_KEY`.

## Flash

```sh
cargo build --release -p sht3x-ssd1306
```

The first flash goes over USB with the OTA layout (see
[`../ota-layout/README.md`](../ota-layout/README.md)); later builds are
published as releases with `scripts/publish.sh` from ull-fleet-rs.

## Adding metrics

Set a value from any task; the agent sends the latest values at each check-in:

```rust
METRICS.set("pressure_hpa", 1013.2);
```

Names are `[a-z0-9_]`, at most 32 characters, with the unit as suffix
(`_c`, `_pct`, `_hpa`, `_ppm`, `_v`, ...); the server derives labels and units
from it. Raise the `Metrics<N>` capacity in `app.rs` for more than two.
