include!("../build_support.rs");

fn main() {
    rerun_if_changed(&[
        "../../crates",
        "../../boards",
        "../../../ull-drivers-rs/drivers/ull-sht3x/src",
        "../../../ull-drivers-rs/drivers/ull-ssd1306/src",
    ]);
    load_dotenv();
    export_firmware_identity();
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
