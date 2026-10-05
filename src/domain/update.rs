//! The updater's vocabulary and its rules.
//!
//! Pure: no HTTP, no filesystem, no egui. What lives here is what "is this
//! release newer", "which asset belongs to this machine" and "what should the
//! splash be showing" mean — the parts that are worth testing and that must not
//! be re-derived by the adapter.
//!
//! The state machine has one deliberate asymmetry, and it is the whole of D5: a
//! failed **check** is not a failure the user sees, because they never asked for
//! it. A failed **download** is, because by then something was visibly under
//! way. [`UpdateState::from_check_error`] and [`UpdateState::Failed`] are the two
//! sides of that.

use std::fmt;

/// Where the update flow has got to. The splash renders exactly this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    /// Nothing has been asked for yet — the updater is disabled, or this is a
    /// manual check that has not been started.
    Idle,
    /// Asking GitHub what the latest release is.
    Checking,
    /// Nothing to do. Also where a *failed check* lands (D5).
    UpToDate,
    /// Fetching the asset. `total` is `None` only if the server declined to say
    /// how big it is, which leaves the bar indeterminate rather than wrong.
    Downloading {
        version: String,
        received: u64,
        total: Option<u64>,
    },
    /// Unpacking and swapping. Not cancellable — this is the window in which the
    /// installed app is neither wholly the old one nor wholly the new one.
    Installing { version: String },
    /// Installed; the process is about to hand over to the new build.
    Restarting { version: String },
    /// The download or install failed. Carries what to tell the user, and is the
    /// only state that offers "Continue anyways".
    Failed { message: String },
}

impl UpdateState {
    /// What a failed *version check* becomes.
    ///
    /// [`UpToDate`](Self::UpToDate), never [`Failed`](Self::Failed): the user did
    /// not ask to check for updates on launch, so being offline must not put an
    /// error in front of them or make them click past one to reach their work
    /// (D5). The reason is logged by the caller and otherwise dropped.
    pub fn from_check_error() -> Self {
        Self::UpToDate
    }

    /// Whether the splash has finished with this state and the app may proceed.
    /// `Failed` is excluded: it waits for "Continue anyways".
    pub fn is_settled(&self) -> bool {
        matches!(self, Self::UpToDate)
    }

    /// Whether the user needs to acknowledge this before the app can continue.
    pub fn needs_acknowledgement(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Whether an update is being fetched or applied. While this is true the app
    /// must not be quit out from under it.
    pub fn is_working(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Installing { .. }
        )
    }

    /// The line under the progress bar.
    pub fn status_line(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Checking => "Checking for updates…".to_owned(),
            Self::UpToDate => "Up to date".to_owned(),
            Self::Downloading { version, .. } => format!("Downloading v{version}…"),
            Self::Installing { version } => format!("Installing v{version}…"),
            Self::Restarting { version } => format!("Restarting into v{version}…"),
            Self::Failed { message } => message.clone(),
        }
    }

    /// How full the progress bar should be, or `None` for an indeterminate one.
    ///
    /// Clamped rather than trusted: a server that reports a `Content-Length`
    /// smaller than what it then sends would otherwise overfill the track.
    pub fn progress(&self) -> Option<f32> {
        match self {
            Self::Downloading {
                received,
                total: Some(total),
                ..
            } if *total > 0 => Some((*received as f32 / *total as f32).clamp(0.0, 1.0)),
            _ => None,
        }
    }

    /// The `12.4 / 31.8 MB` readout, when there is one to give.
    pub fn byte_readout(&self) -> Option<String> {
        match self {
            Self::Downloading {
                received,
                total: Some(total),
                ..
            } => Some(format!("{} / {}", megabytes(*received), megabytes(*total))),
            Self::Downloading {
                received,
                total: None,
                ..
            } => Some(megabytes(*received)),
            _ => None,
        }
    }
}

/// One decimal place of MB — enough to show movement, not so much that the
/// last digit flickers.
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// A release worth installing, resolved down to the one asset this machine can
/// actually use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// Semver without the leading `v`.
    pub version: String,
    pub asset_name: String,
    pub asset_url: String,
    /// Byte size, for a determinate progress bar (D9). `None` when the release
    /// metadata omitted it.
    pub size: Option<u64>,
}

/// Everything that can go wrong, in terms the UI can show a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// The updater is switched off for this build (D3).
    Disabled,
    /// Could not reach GitHub, or it answered with something unusable.
    Network(String),
    /// A release exists but carries nothing for this platform.
    NoAssetForTarget { version: String, target: String },
    /// The download did not complete.
    Download(String),
    /// The archive could not be unpacked.
    Extract(String),
    /// The new build could not be put into place.
    Install(String),
    /// The app is not laid out the way this platform's installer expects — on
    /// macOS, running outside a `.app` bundle.
    Unsupported(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "Updates are disabled for this build."),
            Self::Network(detail) => write!(f, "Could not check for updates: {detail}"),
            Self::NoAssetForTarget { version, target } => {
                write!(f, "v{version} has no download for {target}.")
            }
            Self::Download(detail) => write!(f, "Download failed: {detail}"),
            Self::Extract(detail) => write!(f, "Could not unpack the update: {detail}"),
            Self::Install(detail) => write!(f, "Could not install the update: {detail}"),
            Self::Unsupported(detail) => write!(f, "Cannot update this install: {detail}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Whether `candidate` is a release we should move to.
///
/// Strictly greater, so a downgrade is never installed (S5) and re-running the
/// same version is a no-op. Anything unparseable is treated as "not newer": a
/// tag nobody can order is not a tag to replace a working build with.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

/// `v1.2.3` / `1.2.3` → `(1, 2, 3)`. Pre-release and build suffixes are dropped
/// after the patch number, so `1.2.3-rc1` orders as `1.2.3`; GitHub's
/// `releases/latest` excludes pre-releases anyway (S1).
fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let trimmed = version.trim().trim_start_matches(['v', 'V']);
    let core = trimmed
        .split(['-', '+'])
        .next()
        .filter(|core| !core.is_empty())?;

    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    // A missing minor or patch reads as zero, so `v2` compares against `1.9.0`.
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    // `1.2.3.4` is not a version this app publishes.
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// The asset name this build expects, from the configured pattern.
///
/// The `target` is the platform moniker in [`current_target`], not Rust's full
/// triple: release assets are named for humans reading a downloads list.
pub fn asset_name(pattern: &str, version: &str, target: &str) -> String {
    pattern
        .replace("{version}", version.trim_start_matches(['v', 'V']))
        .replace("{target}", target)
}

/// How this build names itself in a release asset.
///
/// Resolved at compile time from the build target, which is what makes AC 8
/// hold without asking the OS anything at runtime.
pub const fn current_target() -> &'static str {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-aarch64"
        } else {
            "macos-x86_64"
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "windows-aarch64"
        } else {
            "windows-x86_64"
        }
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    }
}

/// The archive extension this platform's assets use. Windows ships `.zip`
/// because that is what Windows can open without extra tools; the Unixes ship
/// `.tar.gz`.
pub const fn archive_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".zip"
    } else {
        ".tar.gz"
    }
}

/// Picks this machine's asset out of everything a release carries.
///
/// Matches on a prefix rather than the whole name so a release may attach
/// checksums (`…-macos-aarch64.tar.gz.sha256`) without them being mistaken for
/// the download — the extension check is what rules those out.
pub fn select_asset<'a, I>(assets: I, expected: &str) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let extension = archive_extension();
    assets
        .into_iter()
        .find(|name| name.starts_with(expected) && name.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Version ordering (S5) --------------------------------------------

    #[test]
    fn only_a_strictly_greater_version_is_newer() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.9.9", "1.0.0"));

        assert!(!is_newer("0.1.0", "0.1.0"), "same version is not an update");
        assert!(!is_newer("0.2.0", "0.1.9"), "downgrades are refused");
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn the_v_prefix_is_optional_on_either_side() {
        assert!(is_newer("v0.1.0", "v0.1.1"));
        assert!(is_newer("0.1.0", "v0.1.1"));
        assert!(is_newer("v0.1.0", "0.1.1"));
    }

    #[test]
    fn a_version_that_cannot_be_ordered_is_never_newer() {
        // Better to skip an update than to replace a working build on the
        // strength of a tag we do not understand.
        assert!(!is_newer("0.1.0", "latest"));
        assert!(!is_newer("0.1.0", ""));
        assert!(!is_newer("0.1.0", "1.2.3.4"));
        assert!(!is_newer("nightly", "0.2.0"));
    }

    #[test]
    fn missing_components_read_as_zero() {
        assert!(is_newer("1.9.0", "2"));
        assert!(is_newer("1.0.0", "1.1"));
        assert!(!is_newer("1.0.0", "1"));
    }

    #[test]
    fn a_prerelease_suffix_orders_by_its_core() {
        assert!(is_newer("0.1.0", "0.2.0-rc1"));
        assert!(!is_newer("0.2.0", "0.2.0-rc1"));
    }

    // -- Asset resolution (AC 8) ------------------------------------------

    #[test]
    fn the_asset_pattern_expands_both_placeholders() {
        assert_eq!(
            asset_name("HeadroomLab-{version}-{target}", "1.2.0", "macos-aarch64"),
            "HeadroomLab-1.2.0-macos-aarch64"
        );
        // The tag carries a `v`; the file name does not.
        assert_eq!(
            asset_name("HeadroomLab-{version}-{target}", "v1.2.0", "linux-x86_64"),
            "HeadroomLab-1.2.0-linux-x86_64"
        );
    }

    #[test]
    fn the_target_moniker_matches_the_build() {
        let target = current_target();
        if cfg!(target_os = "macos") {
            assert!(target.starts_with("macos-"));
        } else if cfg!(target_os = "windows") {
            assert!(target.starts_with("windows-"));
        } else {
            assert!(target.starts_with("linux-"));
        }
    }

    #[test]
    fn select_asset_finds_this_platforms_archive() {
        let expected = format!("HeadroomLab-1.2.0-{}", current_target());
        let archive = format!("{expected}{}", archive_extension());
        let assets = [
            "HeadroomLab-1.2.0-some-other-target.tar.gz".to_owned(),
            archive.clone(),
        ];
        let names: Vec<&str> = assets.iter().map(String::as_str).collect();
        assert_eq!(select_asset(names, &expected), Some(archive.as_str()));
    }

    #[test]
    fn select_asset_ignores_checksums_beside_the_real_download() {
        let expected = format!("HeadroomLab-1.2.0-{}", current_target());
        let archive = format!("{expected}{}", archive_extension());
        let checksum = format!("{archive}.sha256");
        // Checksum first, so a naive prefix match would pick it.
        let assets = [checksum, archive.clone()];
        let names: Vec<&str> = assets.iter().map(String::as_str).collect();
        assert_eq!(select_asset(names, &expected), Some(archive.as_str()));
    }

    #[test]
    fn select_asset_returns_none_when_this_platform_is_missing() {
        let assets = vec!["HeadroomLab-1.2.0-plan9-riscv.tar.gz"];
        let expected = format!("HeadroomLab-1.2.0-{}", current_target());
        assert_eq!(select_asset(assets, &expected), None);
    }

    // -- The state machine (D5) -------------------------------------------

    /// The asymmetry the whole of D5 rests on.
    #[test]
    fn a_failed_check_is_not_shown_to_the_user() {
        let state = UpdateState::from_check_error();
        assert_eq!(state, UpdateState::UpToDate);
        assert!(state.is_settled(), "the splash moves on");
        assert!(!state.needs_acknowledgement(), "and asks for nothing");
    }

    /// A failed download, by contrast, is (AC 5).
    #[test]
    fn a_failed_download_waits_for_the_user() {
        let state = UpdateState::Failed {
            message: UpdateError::Download("connection reset".into()).to_string(),
        };
        assert!(state.needs_acknowledgement());
        assert!(!state.is_settled(), "it must not advance on its own");
        assert!(state.status_line().contains("connection reset"));
    }

    #[test]
    fn only_the_working_states_report_as_working() {
        assert!(UpdateState::Checking.is_working());
        assert!(
            UpdateState::Downloading {
                version: "1.0.0".into(),
                received: 1,
                total: Some(2),
            }
            .is_working()
        );
        assert!(
            UpdateState::Installing {
                version: "1.0.0".into()
            }
            .is_working()
        );

        assert!(!UpdateState::Idle.is_working());
        assert!(!UpdateState::UpToDate.is_working());
        assert!(
            !UpdateState::Failed {
                message: "x".into()
            }
            .is_working()
        );
    }

    // -- Progress (AC 2) --------------------------------------------------

    #[test]
    fn progress_is_the_fraction_downloaded() {
        let half = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 50,
            total: Some(100),
        };
        assert_eq!(half.progress(), Some(0.5));
        assert_eq!(half.byte_readout().unwrap(), "0.0 MB / 0.0 MB");
    }

    #[test]
    fn a_zero_total_does_not_divide_by_zero() {
        let state = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 0,
            total: Some(0),
        };
        assert_eq!(state.progress(), None, "indeterminate, not NaN");
    }

    #[test]
    fn overshooting_the_reported_size_clamps_to_full() {
        let state = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 300,
            total: Some(100),
        };
        assert_eq!(state.progress(), Some(1.0));
    }

    #[test]
    fn an_unknown_total_leaves_the_bar_indeterminate_but_still_counts_bytes() {
        let state = UpdateState::Downloading {
            version: "1.0.0".into(),
            received: 5 * 1024 * 1024,
            total: None,
        };
        assert_eq!(state.progress(), None);
        assert_eq!(state.byte_readout().unwrap(), "5.0 MB");
    }

    #[test]
    fn states_without_a_download_have_no_bar_or_readout() {
        for state in [
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::UpToDate,
            UpdateState::Installing {
                version: "1.0.0".into(),
            },
        ] {
            assert_eq!(state.progress(), None);
            assert_eq!(state.byte_readout(), None);
        }
    }

    #[test]
    fn status_lines_name_the_version_being_installed() {
        assert_eq!(UpdateState::Checking.status_line(), "Checking for updates…");
        assert_eq!(
            UpdateState::Downloading {
                version: "1.2.0".into(),
                received: 0,
                total: None,
            }
            .status_line(),
            "Downloading v1.2.0…"
        );
        assert_eq!(
            UpdateState::Installing {
                version: "1.2.0".into()
            }
            .status_line(),
            "Installing v1.2.0…"
        );
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            UpdateError::NoAssetForTarget {
                version: "1.2.0".into(),
                target: "linux-aarch64".into(),
            }
            .to_string(),
            "v1.2.0 has no download for linux-aarch64."
        );
        assert!(UpdateError::Disabled.to_string().ends_with('.'));
    }
}
