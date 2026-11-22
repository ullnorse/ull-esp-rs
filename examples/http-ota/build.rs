include!("../build_support.rs");

fn main() {
    rerun_if_changed(&["../../crates", "../../boards"]);
    load_dotenv();
    export_firmware_identity();
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
