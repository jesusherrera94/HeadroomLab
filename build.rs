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

fn main() {
    for var in CONFIG_VARS {
        println!("cargo::rerun-if-env-changed={var}");
    }
    println!("cargo::rerun-if-changed=build.rs");
}
