//! Replacing the installed app with the unpacked update: the whole `.app`
//! bundle on macOS, the bare executable elsewhere.

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use super::location::macos_bundle_root;
use crate::domain::update::UpdateError;

#[cfg(target_os = "macos")]
pub(super) fn install(unpacked: &Path) -> Result<(), UpdateError> {
    let exe = std::env::current_exe()
        .map_err(|e| UpdateError::Unsupported(format!("cannot locate this executable: {e}")))?;

    let current = macos_bundle_root(&exe).ok_or_else(|| {
        UpdateError::Unsupported(
            "this build is not running from a HeadroomLab.app bundle, so it cannot replace itself. \
             Download the latest version manually."
                .into(),
        )
    })?;

    let new_bundle = find_bundle(unpacked)?;
    swap_bundle(&current, &new_bundle)
}

#[cfg(target_os = "macos")]
fn swap_bundle(current: &Path, new_bundle: &Path) -> Result<(), UpdateError> {
    let parent = current
        .parent()
        .ok_or_else(|| UpdateError::Install("the app has no parent directory".into()))?;
    if is_read_only(parent) {
        return Err(UpdateError::Install(format!(
            "{} is not writable. Move HeadroomLab somewhere you own, or reinstall it manually.",
            parent.display()
        )));
    }

    let retired = current.with_extension("app.old");
    let _ = fs::remove_dir_all(&retired);
    fs::rename(current, &retired)
        .map_err(|e| UpdateError::Install(format!("could not move the old app aside: {e}")))?;

    if let Err(e) = fs::rename(new_bundle, current) {
        let _ = fs::rename(&retired, current);
        return Err(UpdateError::Install(format!(
            "could not move the new app into place: {e}"
        )));
    }

    let _ = fs::remove_dir_all(&retired);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(super) fn install(unpacked: &Path) -> Result<(), UpdateError> {
    let binary = find_executable(unpacked)?;
    self_update::self_replace::self_replace(&binary)
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_bundle(root: &Path) -> Result<PathBuf, UpdateError> {
    fn scan(dir: &Path, depth: usize) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;
        let mut nested = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("app"))
            {
                return Some(path);
            }
            if path.is_dir() {
                nested.push(path);
            }
        }
        if depth == 0 {
            return None;
        }
        nested.into_iter().find_map(|dir| scan(&dir, depth - 1))
    }

    scan(root, 1)
        .ok_or_else(|| UpdateError::Extract("the download contained no HeadroomLab.app".into()))
}

#[cfg(not(target_os = "macos"))]
fn find_executable(root: &Path) -> Result<PathBuf, UpdateError> {
    let wanted = if cfg!(target_os = "windows") {
        "headroomlab.exe"
    } else {
        "headroomlab"
    };

    fn scan(dir: &Path, wanted: &str, depth: usize) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;
        let mut nested = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                nested.push(path);
                continue;
            }
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
            {
                return Some(path);
            }
        }
        if depth == 0 {
            return None;
        }
        nested
            .into_iter()
            .find_map(|dir| scan(&dir, wanted, depth - 1))
    }

    scan(root, wanted, 1)
        .ok_or_else(|| UpdateError::Extract(format!("the download contained no {wanted}")))
}

#[cfg(target_os = "macos")]
fn is_read_only(dir: &Path) -> bool {
    fs::metadata(dir).is_ok_and(|m| m.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn the_executable_is_found_at_the_archive_root_or_one_level_in() {
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(target_os = "windows") {
            "HeadroomLab.exe"
        } else {
            "HeadroomLab"
        };

        let nested = dir.path().join("HeadroomLab-1.0.0-linux-x86_64");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(name), b"binary").unwrap();

        assert_eq!(find_executable(dir.path()).unwrap(), nested.join(name));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn an_archive_without_the_binary_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("README.md"), b"nope").unwrap();
        assert!(matches!(
            find_executable(dir.path()),
            Err(UpdateError::Extract(_))
        ));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn the_bundle_is_found_at_the_archive_root_or_one_level_in() {
        let dir = tempfile::tempdir().unwrap();

        let nested = dir.path().join("HeadroomLab-1.0.0-macos-aarch64");
        let bundle = nested.join("HeadroomLab.app");
        fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();

        assert_eq!(find_bundle(dir.path()).unwrap(), bundle);
    }

    #[cfg(target_os = "macos")]
    fn fake_bundle(at: &Path, marker: &str) -> PathBuf {
        let bundle = at.join("HeadroomLab.app");
        fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        fs::write(bundle.join("Contents/MacOS/HeadroomLab"), marker).unwrap();
        bundle
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn swapping_replaces_the_bundle_and_clears_up_after_itself() {
        let installed = tempfile::tempdir().unwrap();
        let staged = tempfile::tempdir().unwrap();

        let current = fake_bundle(installed.path(), "old");
        let incoming = fake_bundle(staged.path(), "new");

        swap_bundle(&current, &incoming).unwrap();

        assert_eq!(
            fs::read_to_string(current.join("Contents/MacOS/HeadroomLab")).unwrap(),
            "new",
            "the new build is at the path the old one occupied"
        );
        assert!(
            !current.with_extension("app.old").exists(),
            "the retired bundle is cleaned up"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_failed_swap_puts_the_old_bundle_back() {
        let installed = tempfile::tempdir().unwrap();
        let current = fake_bundle(installed.path(), "old");

        let missing = installed.path().join("nowhere/HeadroomLab.app");

        let result = swap_bundle(&current, &missing);

        assert!(matches!(result, Err(UpdateError::Install(_))));
        assert_eq!(
            fs::read_to_string(current.join("Contents/MacOS/HeadroomLab")).unwrap(),
            "old",
            "the working app was restored"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_leftover_retired_bundle_does_not_block_a_later_swap() {
        let installed = tempfile::tempdir().unwrap();
        let staged = tempfile::tempdir().unwrap();

        let current = fake_bundle(installed.path(), "old");
        let incoming = fake_bundle(staged.path(), "new");

        let stale = current.with_extension("app.old");
        fs::create_dir_all(stale.join("Contents/MacOS")).unwrap();
        fs::write(stale.join("Contents/MacOS/HeadroomLab"), "ancient").unwrap();

        swap_bundle(&current, &incoming).unwrap();
        assert_eq!(
            fs::read_to_string(current.join("Contents/MacOS/HeadroomLab")).unwrap(),
            "new"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn an_archive_without_a_bundle_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("HeadroomLab"), b"bare binary").unwrap();
        assert!(matches!(
            find_bundle(dir.path()),
            Err(UpdateError::Extract(_))
        ));
    }
}
