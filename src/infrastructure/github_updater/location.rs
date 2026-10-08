//! Where the running app lives on disk, and relaunching it once an update is
//! in place.

use std::path::{Path, PathBuf};

use crate::domain::update::UpdateError;

pub(super) fn staging_parent() -> Result<PathBuf, UpdateError> {
    let exe = std::env::current_exe()
        .map_err(|e| UpdateError::Unsupported(format!("cannot locate this executable: {e}")))?;

    let anchor = macos_bundle_root(&exe).unwrap_or(exe);
    anchor
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| UpdateError::Unsupported("this executable has no parent directory".into()))
}

pub(super) fn macos_bundle_root(exe: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let root = exe.ancestors().nth(3)?;
    (root.extension()?.eq_ignore_ascii_case("app")).then(|| root.to_path_buf())
}

pub(super) fn restart_now() -> Result<std::convert::Infallible, UpdateError> {
    use std::process::Command;

    let exe = std::env::current_exe()
        .map_err(|e| UpdateError::Unsupported(format!("cannot locate this executable: {e}")))?;

    #[cfg(target_os = "macos")]
    if let Some(bundle) = macos_bundle_root(&exe) {
        Command::new("/usr/bin/open")
            .arg("-n")
            .arg(&bundle)
            .spawn()
            .map_err(|e| UpdateError::Install(format!("could not relaunch: {e}")))?;
        std::process::exit(0);
    }

    Command::new(&exe)
        .spawn()
        .map_err(|e| UpdateError::Install(format!("could not relaunch: {e}")))?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn only_a_dot_app_ancestor_counts_as_a_bundle() {
        assert_eq!(
            macos_bundle_root(Path::new(
                "/Apps/HeadroomLab.app/Contents/MacOS/HeadroomLab"
            )),
            Some(PathBuf::from("/Apps/HeadroomLab.app"))
        );
        assert_eq!(
            macos_bundle_root(Path::new("/home/me/build/bin/HeadroomLab")),
            None
        );
        assert_eq!(macos_bundle_root(Path::new("/HeadroomLab")), None);
    }

    #[test]
    fn staging_sits_beside_the_thing_being_replaced() {
        let parent = staging_parent().expect("this test binary has a parent directory");
        let exe = std::env::current_exe().unwrap();
        let anchor = macos_bundle_root(&exe).unwrap_or(exe);
        assert_eq!(parent, anchor.parent().unwrap());
    }
}
