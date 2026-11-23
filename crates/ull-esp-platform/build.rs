//! Bakes the shared device defaults into the firmware: Wi-Fi, fleet server
//! and the update key. They are read once for every project, from
//! `~/.config/ull-esp/device.env` (or the file `ULL_ESP_CONFIG` names).
//! A variable set in the environment wins over the file.

use std::path::PathBuf;

const KEYS: [&str; 4] = [
    "WIFI_SSID",
    "WIFI_PASSWORD",
    "FLEET_BASE_URL",
    "OTA_PUBLIC_KEY",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ULL_ESP_CONFIG");
    for key in KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }

    let path = match std::env::var_os("ULL_ESP_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".config/ull-esp/device.env"),
            None => return,
        },
    };
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(contents) = std::fs::read_to_string(&path) else {
        println!(
            "cargo:warning={} not found: no built-in Wi-Fi, fleet server or update key",
            path.display()
        );
        return;
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !KEYS.contains(&key) || std::env::var_os(key).is_some() {
            continue;
        }
        let mut value = value.trim();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = &value[1..value.len() - 1];
        }
        println!("cargo:rustc-env={key}={value}");
    }
}
