//! Runs before compiling COAT.
//!
//! - Rebuilds when the built-in simlog addresses change (they are read at compile time
//!   with `option_env!`, see `src/source.rs`).
//! - On Windows, gives COAT.exe its icon and version details.

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/coat.ico");
    println!("cargo:rerun-if-env-changed=COAT_SIMLOG_NORDIC");
    println!("cargo:rerun-if-env-changed=COAT_SIMLOG_UAE");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("packaging/windows/coat.ico");
        resource.set("ProductName", "COAT");
        resource.set("FileDescription", "COAT: call overview and timeline");
        resource.compile().expect("embedding the Windows icon");
    }
}
