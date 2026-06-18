# http-ota

Dedicated ESP32 OTA example using `esp-hal`, `embassy-net`, `picoserve`, `esp-storage`, and `esp-bootloader-esp-idf`.

It exposes:

- `POST /ota` for authenticated application-image uploads
- `GET /ota/status` for authenticated OTA status and upload progress

## Configuration

Copy `.env.example` to `.env` and set:

- `WIFI_SSID`
- `WIFI_PASSWORD`
- `OTA_TOKEN`
- optional `OTA_PORT` (defaults to `8080`)

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

Create an application image for OTA upload:

```bash
espflash save-image --chip esp32 \
  --partition-table examples/http-ota/partitions.csv \
  --target-app-partition ota_0 \
  target/xtensa-esp32-none-elf/release/http-ota \
  ota.bin
```

Do not pass `--merge`. OTA uploads must use the application image only.

## Upload Firmware

Compute the SHA-256 digest and upload the image:

```bash
SHA256=$(sha256sum ota.bin | cut -d' ' -f1)

curl -X POST "http://<device-ip>:8080/ota" \
  -H "X-OTA-Token: <your-token>" \
  -H "X-OTA-SHA256: ${SHA256}" \
  --data-binary @ota.bin
```

Check status:

```bash
curl -H "X-OTA-Token: <your-token>" "http://<device-ip>:8080/ota/status"
```

## Bootloader Note

The checked-in `bootloader.bin` is pinned so the flashing workflow is reproducible from this repo.

If your board does not show the expected rollback behavior, replace it with an ESP-IDF bootloader built with rollback enabled (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`).
