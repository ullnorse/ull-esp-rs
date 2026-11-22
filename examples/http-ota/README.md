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
- `OTA_PUBLIC_KEY`: the Ed25519 public key updates must be signed with (64 hex
  digits). The build fails without it.

Create the key pair once with the signer from the sibling `ull-fleet-rs` repo,
and keep the secret key out of both repos:

```bash
cargo run --release -p ull-fleet-sign -- keygen ~/.config/ull-fleet/ota-signing.key
# prints the public key: put it in .env as OTA_PUBLIC_KEY
```

For this POC, `FLEET_BASE_URL` must use a numeric IPv4 address. HTTPS and DNS are not wired yet.

These values are compiled into the firmware by `build.rs`. Changing them
requires a rebuild.

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

## Factoryless OTA Layout

`examples/http-ota/partitions.csv` uses a factoryless layout:

- no `factory` app partition
- one `otadata` partition
- two OTA app slots: `ota_0` and `ota_1`

On first boot, the example seeds `otadata` from the currently booted OTA slot so
the ESP-IDF bootloader can manage future OTA selections normally.

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

## Backend Contract

The `ull-fleet-rs` backend POC in the sibling repo exposes:

- `POST /api/upload`
- `GET /api/update`

The current device client expects `GET /api/update` to behave like this:

- `204 No Content` means no update is available
- any update response must be `2xx`
- the response body must be the raw Application Image bytes
- the response must include `Content-Length`
- the response must include `X-Application-Image-Sha256`
- the response must include `X-Application-Image-Ed25519`: the signature from
  `ull-fleet-sign sign` (128 hex digits)
- chunked transfer encoding is not supported

The poll request currently sends only `GET /api/update` with a `Host` header. It
does not send device identity, current firmware version, or update
authorization.

## Backend Flow

Sign and upload a new image with:

```bash
curl -H "Content-Type: application/octet-stream" \
     -H "X-Application-Image-Ed25519: $(ull-fleet-sign sign ~/.config/ull-fleet/ota-signing.key ota.bin)" \
     --data-binary @ota.bin http://127.0.0.1:3000/api/upload
```

The device will pick it up on the next 10-second poll.

## Health Confirmation

On a `pending_verify` or `new` boot, the example waits up to 30 seconds for
network configuration to come back and then marks the running image healthy.

If that timeout expires, or if health confirmation itself fails, the example
marks the running image invalid and reboots so the bootloader can roll back.

Rollback remains driven by the ESP-IDF 2nd-stage bootloader state.

## Install Checks

In order, before anything is activated:

1. The Ed25519 signature over the claimed SHA-256 verifies against
   `OTA_PUBLIC_KEY`. Nothing is erased if it does not.
2. The ESP image header and app descriptor magic, from the first 36 bytes.
3. The byte count, and the SHA-256 of what was written equals the signed digest.

The target slot is erased one 4 KiB sector at a time, right before each sector
is written. If a download fails part-way, only the first sector of the target
slot is erased, so the partial image cannot pass the bootloader's header check;
the running image stays selected.

## Security Note

Images are signed offline and the device checks the signature with a key built
into the firmware, so the fleet server cannot push an image the device accepts
unless it was signed with the secret key. Transport is still plain HTTP, so the
download can be observed, and an attacker can withhold or replay an older
signed image (there is no version check yet). Flash encryption and secure boot
are not enabled.

## Bootloader Note

The checked-in `bootloader.bin` is pinned so the flashing workflow is reproducible from this repo.

If your board does not show the expected rollback behavior, replace it with an ESP-IDF bootloader built with rollback enabled (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`).
