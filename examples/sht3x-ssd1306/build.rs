include!("../build_support.rs");

fn main() {
    println!("cargo:rerun-if-changed=.env");
    load_dotenv();
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn load_dotenv() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let env_path = manifest_dir.join(".env");

    let Ok(contents) = std::fs::read_to_string(&env_path) else {
        println!("cargo:warning=.env not found; Wi-Fi variables must come from the environment");
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
