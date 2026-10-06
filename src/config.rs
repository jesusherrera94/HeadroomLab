//! Build-time configuration: the handful of values that differ between a
//! developer's machine, a fork and a published release.
//!
//! A sibling of the four layers rather than a member of one, because this is
//! *build* configuration, not domain vocabulary — and every layer may read it.
//! Reading one of these from `domain` is not an outward dependency: `option_env!`
//! is evaluated by the compiler, so each of these is a literal by the time any
//! code runs. There is no file to ship, nothing to parse and nothing to lose.
//!
//! Overriding means rebuilding:
//!
//! ```text
//! HL_UPDATE_ENABLED=1 cargo run
//! HL_UPDATE_REPO_OWNER=myfork cargo build --release
//! ```
//!
//! Cargo does not re-run the build on an env-var change by itself, so a
//! `build.rs` re-run directive keeps that honest — see `build.rs`.

pub const UPDATE_REPO_OWNER: &str = match option_env!("HL_UPDATE_REPO_OWNER") {
    Some(value) => value,
    None => "jesusherrera94",
};

pub const UPDATE_REPO_NAME: &str = match option_env!("HL_UPDATE_REPO_NAME") {
    Some(value) => value,
    None => "HeadroomLab",
};

pub const UPDATE_TIMEOUT_SECS: u64 = match option_env!("HL_UPDATE_TIMEOUT_SECS") {
    Some(value) => match u64::from_str_radix(value, 10) {
        Ok(secs) => secs,
        Err(_) => 5,
    },
    None => 5,
};

pub const UPDATE_ASSET_PATTERN: &str = match option_env!("HL_UPDATE_ASSET_PATTERN") {
    Some(value) => value,
    None => "HeadroomLab-{version}-{target}",
};

pub const HELP_URL: &str = match option_env!("HL_HELP_URL") {
    Some(value) => value,
    None => "https://github.com/jesusherrera94/HeadroomLab#readme",
};

pub const GIT_FOR_WINDOWS_URL: &str = match option_env!("HL_GIT_FOR_WINDOWS_URL") {
    Some(value) => value,
    None => "https://git-scm.com/download/win",
};

pub const UPDATE_ENABLED: bool = match option_env!("HL_UPDATE_ENABLED") {
    Some(value) => matches!(value.as_bytes(), b"1" | b"true" | b"TRUE" | b"yes"),
    None => !cfg!(debug_assertions),
};

const _: () = assert!(
    UPDATE_TIMEOUT_SECS > 0,
    "a zero check timeout would give up before asking"
);
const _: () = assert!(
    !UPDATE_REPO_OWNER.is_empty() && !UPDATE_REPO_NAME.is_empty(),
    "an empty owner or repo would query api.github.com/repos//releases/latest"
);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_populated() {
        assert!(!UPDATE_REPO_OWNER.is_empty());
        assert!(!UPDATE_REPO_NAME.is_empty());
        assert!(HELP_URL.starts_with("https://"));
        assert!(GIT_FOR_WINDOWS_URL.starts_with("https://"));
    }

    #[test]
    fn the_asset_pattern_carries_both_placeholders() {
        assert!(UPDATE_ASSET_PATTERN.contains("{version}"));
        assert!(UPDATE_ASSET_PATTERN.contains("{target}"));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn updates_are_off_by_default_in_debug() {
        let overridden = option_env!("HL_UPDATE_ENABLED").is_some();
        let enabled = UPDATE_ENABLED;
        assert!(overridden || !enabled, "a debug build must not self-update");
    }
}
