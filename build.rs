//! Makes `src/config.rs`'s `option_env!` overrides actually take effect.
//!
//! Cargo does not watch environment variables by default, so changing
//! `HL_UPDATE_ENABLED` between two builds would otherwise reuse the cached
//! artifact and silently keep the old value baked in. Declaring each one here
//! makes Cargo treat it as an input and rebuild when it changes.

const CONFIG_VARS: &[&str] = &[
    "HL_UPDATE_REPO_OWNER",
    "HL_UPDATE_REPO_NAME",
    "HL_UPDATE_TIMEOUT_SECS",
    "HL_UPDATE_ASSET_PATTERN",
    "HL_UPDATE_ENABLED",
    "HL_HELP_URL",
    "HL_GIT_FOR_WINDOWS_URL",
];

/// Embeds the application icon into the Windows executable.
///
/// Windows has no bundle and no manifest file beside the binary — an `.exe`'s
/// icon lives *inside* it as a resource, so this is the only way Explorer, the
/// taskbar or alt-tab will show anything but the generic icon. macOS gets its
/// icon from the `.app` bundle and Linux from its `.desktop` entry, so neither
/// needs anything here.
#[cfg(windows)]
fn embed_windows_icon() {
    const ICON: &str = "packaging/windows/AppIcon.ico";

    if !std::path::Path::new(ICON).exists() {
        // Not fatal: a missing icon is a cosmetic problem, and failing the build
        // over one would be worse than shipping the generic icon.
        println!("cargo::warning={ICON} not found — the .exe will use the generic icon");
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(ICON);
    if let Err(e) = resource.compile() {
        println!("cargo::warning=could not embed the Windows icon: {e}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {}

fn main() {
    for var in CONFIG_VARS {
        println!("cargo::rerun-if-env-changed={var}");
    }
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=packaging/windows/AppIcon.ico");

    embed_windows_icon();
}
