#!/usr/bin/env bash
# Rebuilds ../bootloader.bin. Needs ESP-IDF v5.5.1 exported (`. $IDF_PATH/export.sh`).
set -euo pipefail
cd "$(dirname "$0")"
idf.py set-target esp32 >/dev/null
idf.py bootloader
cp build/bootloader/bootloader.bin ../bootloader.bin
