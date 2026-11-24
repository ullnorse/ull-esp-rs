fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    // Xtensa links through GCC, which takes linker flags behind `-Wl,`;
    // RISC-V chips link with rust-lld directly.
    let prefix = if std::env::var("TARGET").is_ok_and(|target| target.starts_with("xtensa")) {
        "-Wl,"
    } else {
        ""
    };
    println!(
        "cargo:rustc-link-arg={prefix}--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}

/// Re-runs the build script when these paths change, so the firmware
/// version below is fresh for every build with different sources.
#[allow(dead_code)]
fn rerun_if_changed(paths: &[&str]) {
    for path in ["build.rs", "Cargo.toml", "src", "../build_support.rs"]
        .iter()
        .chain(paths)
    {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// Sets the release identity read by `ull_esp_platform::firmware_app_desc!`:
///
/// - `FIRMWARE_VERSION`: `<crate version>+<git rev>-<build epoch>` (at most
///   31 bytes, the app descriptor limit). The fleet server keys releases on it.
/// - `FIRMWARE_SECURE_VERSION`: the build epoch. Devices refuse updates with a
///   lower value, so an old signed image cannot be replayed.
/// - `FIRMWARE_BUILD_DATE` / `FIRMWARE_BUILD_TIME`: UTC, `Nov 23 2026` / `03:04:05`.
#[allow(dead_code)]
fn export_firmware_identity() {
    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_secs()
        });
    let rev = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|rev| rev.trim().to_string())
        .filter(|rev| !rev.is_empty())
        .unwrap_or_else(|| "nogit".to_string());
    let mut version = format!(
        "{}+{rev}-{epoch}",
        std::env::var("CARGO_PKG_VERSION").unwrap()
    );
    version.truncate(31);

    let (date, time) = utc_date_time(epoch);
    println!("cargo:rustc-env=FIRMWARE_VERSION={version}");
    println!("cargo:rustc-env=FIRMWARE_SECURE_VERSION={}", epoch.min(u64::from(u32::MAX)));
    println!("cargo:rustc-env=FIRMWARE_BUILD_DATE={date}");
    println!("cargo:rustc-env=FIRMWARE_BUILD_TIME={time}");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

/// `("Nov 23 2026", "03:04:05")` for a Unix time (UTC).
#[allow(dead_code)]
fn utc_date_time(epoch: u64) -> (String, String) {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (epoch / 86_400) as i64;
    let seconds = epoch % 86_400;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        format!("{} {day:02} {year}", MONTHS[(month - 1) as usize]),
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        ),
    )
}
