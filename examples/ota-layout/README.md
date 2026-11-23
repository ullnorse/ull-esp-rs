# OTA layout

Bootloader and partition table shared by the examples that update over the air
(`sht3x-ssd1306`, `http-ota`).

`partitions.csv` has no factory app: `nvs`, `otadata`, `phy_init`, and two
1.94 MB app slots `ota_0` and `ota_1`. On its first boot the fleet agent seeds
`otadata` from the slot it booted, so the bootloader manages later switches.

## First flash

```sh
espflash flash --chip esp32 --monitor \
  --bootloader examples/ota-layout/bootloader.bin \
  --partition-table examples/ota-layout/partitions.csv \
  --erase-parts otadata \
  target/xtensa-esp32-none-elf/release/sht3x-ssd1306
```

`--erase-parts otadata` makes the board boot `ota_0`, where this flash writes.
Without it, an older `otadata` can select the other slot.

## Update images

`scripts/publish.sh` in ull-fleet-rs does this; by hand:

```sh
espflash save-image --chip esp32 \
  --partition-table examples/ota-layout/partitions.csv --target-app-partition ota_0 \
  target/xtensa-esp32-none-elf/release/sht3x-ssd1306 image.bin
ull-fleet-sign sign ~/.config/ull-fleet/ota-signing.key image.bin
```

Do not pass `--merge`: updates are application images only.

## Install checks

`ull_esp_platform::ota::UpdateInstaller`, in order:

1. The Ed25519 signature over the claimed SHA-256 verifies against the key
   built into the firmware. Nothing is erased if it does not.
2. From the first 80 bytes: ESP image and app descriptor magic, and the app
   descriptor `secure_version` is not lower than the running firmware's.
   Nothing is erased if this fails either.
3. Byte count, and the SHA-256 of what was written equals the signed digest.
   Only then is `otadata` switched.

The target slot is erased one 4 KiB sector at a time, right before each write.
If a download stops part-way, the first sector of the target slot is erased, so
the partial image cannot pass the bootloader's header check.

## Health confirmation

After an update the new image boots in `new`; the bootloader moves it to
`pending_verify`. The fleet agent marks it valid after its first successful
check-in. Until then any reset sends the device back to the previous image:

- a panic resets the device (`ull_esp_platform::panic_handler!`);
- a hang trips the hardware watchdog, a stalled agent the software one
  (`ull_esp_platform::watchdog`);
- an image that cannot reach the server within 120 s marks itself invalid and
  reboots.

On the next boot, the bootloader finds the image still `pending_verify`, marks
it aborted, and starts the previous slot. That image reports why: the panic
message or stalled task that the failed image left in RTC memory.

Checked on hardware with an image that panics at boot: three install attempts,
each rolled back to the previous slot and reported as a crash; then a good
image installed, confirmed, and stayed selected across a power reset.

## Bootloader

`bootloader.bin` is an ESP-IDF v5.5.1 second-stage bootloader built from
`bootloader/` with `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`. Rebuild it with
`bootloader/build.sh` after exporting ESP-IDF.

eFuse anti-rollback is off: it burns eFuses, which cannot be undone. The
`secure_version` check above is done by the running firmware instead.

Boards flashed with the earlier bootloader (ESP-IDF v5.5.1-838, no rollback)
need one USB flash to get this one; updates do not replace the bootloader.
