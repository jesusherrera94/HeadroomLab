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

/// GitHub account the updater looks for releases under.
pub const UPDATE_REPO_OWNER: &str = match option_env!("HL_UPDATE_REPO_OWNER") {
    Some(value) => value,
    None => "jesusherrera94",
};

/// Repository the updater looks for releases in.
pub const UPDATE_REPO_NAME: &str = match option_env!("HL_UPDATE_REPO_NAME") {
    Some(value) => value,
    None => "HeadroomLab",
};

/// Seconds the *version check* may take before it is abandoned.
///
/// Deliberately short: a launch-time check is not something the user asked for,
/// so it must never be the reason a splash screen sits there. The download that
/// may follow gets its own, much longer budget.
pub const UPDATE_TIMEOUT_SECS: u64 = match option_env!("HL_UPDATE_TIMEOUT_SECS") {
    Some(value) => match u64::from_str_radix(value, 10) {
        Ok(secs) => secs,
        // A malformed override is a build-time typo. Falling back beats failing
        // the build over a number nobody will notice is wrong.
        Err(_) => 5,
    },
    None => 5,
};

/// Names the release assets the updater will accept, with `{version}` and
/// `{target}` expanded. See `domain::update::asset_name`.
pub const UPDATE_ASSET_PATTERN: &str = match option_env!("HL_UPDATE_ASSET_PATTERN") {
    Some(value) => value,
    None => "HeadroomLab-{version}-{target}",
};

/// Where Help ▸ HeadroomLab Help goes.
pub const HELP_URL: &str = match option_env!("HL_HELP_URL") {
    Some(value) => value,
    None => "https://github.com/jesusherrera94/HeadroomLab#readme",
};

/// Where the terminal sends a Windows user who has no `git` (and therefore no
/// `make`) on their PATH.
pub const GIT_FOR_WINDOWS_URL: &str = match option_env!("HL_GIT_FOR_WINDOWS_URL") {
    Some(value) => value,
    None => "https://git-scm.com/download/win",
};

/// Whether the app checks for updates at all.
///
/// Off in debug and on in release, which is the bypass the developer wants by
/// default: no build of this app taken straight from `cargo run` should reach
/// out to GitHub, still less replace itself. `HL_UPDATE_ENABLED=1` turns it on
/// for a debug build so the flow can be exercised; `0` turns it off for a
/// release build.
pub const UPDATE_ENABLED: bool = match option_env!("HL_UPDATE_ENABLED") {
    Some(value) => matches!(value.as_bytes(), b"1" | b"true" | b"TRUE" | b"yes"),
    None => !cfg!(debug_assertions),
};

// Compile-time guards on the values above. Cheaper and stricter than a test:
// a bad default fails the build rather than a test run, and there is nothing
// runtime about a `const`.
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

    /// The defaults have to be usable as they stand — a blank owner or repo
    /// would send the updater at `api.github.com/repos//releases/latest`.
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

    /// The debug bypass (D3). Without an override, a `cargo run` build must not
    /// be able to reach the network.
    #[test]
    #[cfg(debug_assertions)]
    fn updates_are_off_by_default_in_debug() {
        // Read through a binding so this reflects the built configuration
        // rather than being folded away as a constant.
        let overridden = option_env!("HL_UPDATE_ENABLED").is_some();
        let enabled = UPDATE_ENABLED;
        assert!(overridden || !enabled, "a debug build must not self-update");
    }
}
