include!("../build_support.rs");

fn main() {
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../../crates/ull-esp-platform/src");
    println!("cargo:rerun-if-changed=../../boards/esp32-devkit-v1/src");
    load_dotenv();
    export_build_id();
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn export_build_id() {
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs();
    let git_rev =
        git_output(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "nogit".into());
    let dirty = git_is_dirty();
    let build_id = if dirty {
        format!("{git_rev}-{unix_seconds}-dirty")
    } else {
        format!("{git_rev}-{unix_seconds}")
    };

    println!("cargo:rustc-env=HTTP_OTA_BUILD_ID={build_id}");
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn git_is_dirty() -> bool {
    match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
    {
        Ok(output) if output.status.success() => !output.stdout.is_empty(),
        _ => false,
    }
}

fn load_dotenv() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let env_path = manifest_dir.join(".env");

    let Ok(contents) = std::fs::read_to_string(&env_path) else {
        println!(
            "cargo:warning=.env not found; OTA example variables must come from the environment"
        );
        return;
    };

    for line in contents.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let mut value = raw_value.trim();

        if value.len() >= 2 {
            let double_quoted = value.starts_with('"') && value.ends_with('"');
            let single_quoted = value.starts_with('\'') && value.ends_with('\'');

            if double_quoted || single_quoted {
                value = &value[1..value.len() - 1];
            }
        }

        println!("cargo:rustc-env={key}={value}");
    }
}
