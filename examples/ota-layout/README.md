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

After an update the new image boots in `pending_verify`. The fleet agent marks
it valid after its first successful check-in. If it cannot reach the server
within 120 s, it marks itself invalid and reboots; the bootloader returns to the
previous slot.

## Bootloader

`bootloader.bin` is an ESP-IDF v5.5 second-stage bootloader, pinned so flashing
is reproducible. It skips a slot whose `otadata` state is invalid (checked on
hardware: an update that could not reach the server rolled back to the previous
slot). eFuse anti-rollback is not used; the `secure_version` check above is
done by the running firmware.
