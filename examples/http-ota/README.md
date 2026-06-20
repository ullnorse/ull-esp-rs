# http-ota

Dedicated ESP32 OTA example using `esp-hal`, `embassy-net`, and `ull-esp-platform::ota`.

The device now uses a dirt-simple Device-Pull OTA flow:

- it polls `GET /api/update` every 10 seconds
- if the backend has a pending OTA image, it downloads it over outbound HTTP
- it writes the inactive OTA slot locally and reboots

The example keeps the fleet HTTP contract, polling cadence, and health policy. The reusable ESP image install and boot-state mechanics live in `ull-esp-platform::ota`.

## Configuration

Copy `.env.example` to `.env` and set:

- `WIFI_SSID`
- `WIFI_PASSWORD`
- `FLEET_BASE_URL` as `http://<ipv4>[:port]`

For this POC, `FLEET_BASE_URL` must use a numeric IPv4 address. HTTPS and DNS are not wired yet.

## Initial Flash

Build the example from the workspace root:

```bash
cargo build --release -p http-ota
```

Flash it with the example-scoped bootloader and partition table:

```bash
espflash flash --chip esp32 --monitor \
  --bootloader examples/http-ota/bootloader/bootloader.bin \
  --partition-table examples/http-ota/partitions.csv \
  target/xtensa-esp32-none-elf/release/http-ota
```

## Generate OTA Image

Create an Application Image for the Fleet Management Service to serve:

```bash
espflash save-image --chip esp32 \
  --partition-table examples/http-ota/partitions.csv \
  --target-app-partition ota_0 \
  target/xtensa-esp32-none-elf/release/http-ota \
  ota.bin
```

Do not pass `--merge`. Device-Pull OTA must use the Application Image only.

## Backend Flow

The `ull-fleet-rs` backend POC in the sibling repo exposes:

- `POST /api/upload`
- `GET /api/update`

Upload a new image with:

```bash
curl -F file=@ota.bin http://127.0.0.1:3000/api/upload
```

The device will pick it up on the next 10-second poll.

## Health Confirmation

On a `pending_verify` or `new` boot, the example waits for network restoration and then marks the running image healthy.

Rollback remains driven by the ESP-IDF 2nd-stage bootloader state.

## Bootloader Note

The checked-in `bootloader.bin` is pinned so the flashing workflow is reproducible from this repo.

If your board does not show the expected rollback behavior, replace it with an ESP-IDF bootloader built with rollback enabled (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`).
