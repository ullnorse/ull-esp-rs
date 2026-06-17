include!("../build_support.rs");

fn main() {
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
