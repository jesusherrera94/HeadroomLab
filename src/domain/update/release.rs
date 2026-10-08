//! Deciding whether a published release is newer, and picking this
//! platform's archive from its assets.

pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

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

pub fn asset_name(pattern: &str, version: &str, target: &str) -> String {
    pattern
        .replace("{version}", version.trim_start_matches(['v', 'V']))
        .replace("{target}", target)
}

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

pub const fn archive_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".zip"
    } else {
        ".tar.gz"
    }
}

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

    #[test]
    fn the_asset_pattern_expands_both_placeholders() {
        assert_eq!(
            asset_name("HeadroomLab-{version}-{target}", "1.2.0", "macos-aarch64"),
            "HeadroomLab-1.2.0-macos-aarch64"
        );
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
}
