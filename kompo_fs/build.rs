use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Get the target directory from OUT_DIR
    // OUT_DIR is typically target/release/build/kompo_fs-xxx/out
    // So target/release is 3 levels up
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("Failed to find target directory");

    // Write KOMPO_VFS_VERSION file to target/release (or target/debug)
    let version = env!("CARGO_PKG_VERSION");
    fs::write(target_dir.join("KOMPO_VFS_VERSION"), version)
        .expect("Failed to write KOMPO_VFS_VERSION file");

    // Rerun if Cargo.toml changes
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Link zlib for compression support
    // On macOS, zlib is available as a system library
    // On Linux, it's typically available as libz
    // In the final binary, this will use Ruby's statically linked zlib
    println!("cargo:rustc-link-lib=z");
}
