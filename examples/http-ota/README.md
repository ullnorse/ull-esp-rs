# http-ota

The smallest fleet device: the `ull_esp_platform::fleet` agent plus a blinking
status LED. It sends heartbeats without metrics and installs signed updates. Use
it to try the update path without sensors; `sht3x-ssd1306` is the same agent
with readings.

## Configuration

Copy `.env.example` to `.env`:

- `WIFI_SSID`, `WIFI_PASSWORD`
- `FLEET_BASE_URL`: `http://<ipv4>:<port>` of the ull-fleet server (no DNS, no HTTPS)
- `OTA_PUBLIC_KEY`: 64 hex digits from `ull-fleet-sign keygen`; the build fails without it

Values are compiled in. A variable set in the environment overrides `.env` for
one build.

## Flash and update

```sh
cargo build --release -p http-ota
```

Flash with the OTA layout as described in
[`../ota-layout/README.md`](../ota-layout/README.md). Later builds go out as
releases through the fleet server (`scripts/publish.sh` in ull-fleet-rs).

Every build gets its own version (`0.1.0+<git rev>-<build epoch>`) and
`secure_version` (the build epoch), set by `export_firmware_identity` in
`examples/build_support.rs` and `ull_esp_platform::firmware_app_desc!()`.

## Security

Images are signed offline; the device checks the signature against the key
built into the firmware and refuses images older than itself. Transport is
plain HTTP: a device on the LAN can watch downloads, withhold updates, or post
check-ins for another device id. Flash encryption and secure boot are not
enabled.
