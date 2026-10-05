//! Runs before compiling COAT.
//!
//! - Rebuilds when the build-time settings change: the simlog addresses, the release channel
//!   and the release flag are read at compile time with `option_env!` (see `src/source.rs` and
//!   `src/update.rs`).
//! - On Windows, gives the program its icon and details (green and "COAT Preview" for a
//!   preview build).

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/coat.ico");
    println!("cargo:rerun-if-changed=packaging/windows/coat-preview.ico");
    for variable in ["COAT_SIMLOG_NORDIC", "COAT_SIMLOG_UAE", "COAT_CHANNEL", "COAT_RELEASE_BUILD", "COAT_REPO"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    #[cfg(windows)]
    {
        let preview = std::env::var("COAT_CHANNEL").is_ok_and(|c| c == "preview");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(if preview { "packaging/windows/coat-preview.ico" } else { "packaging/windows/coat.ico" });
        resource.set("ProductName", if preview { "COAT Preview" } else { "COAT" });
        resource.set("FileDescription", if preview {
            "COAT Preview: call overview and timeline (PREVIEW build)"
        } else {
            "COAT: call overview and timeline"
        });
        resource.compile().expect("embedding the Windows icon");
    }
}
