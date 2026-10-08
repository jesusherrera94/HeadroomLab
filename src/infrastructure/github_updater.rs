use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::application::ports::UpdaterPort;
use crate::config;
use crate::domain::update::{
    ReleaseInfo, UpdateError, asset_name, current_target, is_newer, select_asset,
};

const USER_AGENT: &str = concat!("HeadroomLab/", env!("CARGO_PKG_VERSION"));

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct GitHubUpdater {
    owner: String,
    repo: String,
    current_version: String,
}

impl Default for GitHubUpdater {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubUpdater {
    pub fn new() -> Self {
        Self {
            owner: config::UPDATE_REPO_OWNER.to_owned(),
            repo: config::UPDATE_REPO_NAME.to_owned(),
            current_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    fn latest_release_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.repo
        )
    }
}

impl UpdaterPort for GitHubUpdater {
    fn check(&self) -> Result<Option<ReleaseInfo>, UpdateError> {
        if !config::UPDATE_ENABLED {
            return Err(UpdateError::Disabled);
        }

        let body = get_json(
            &self.latest_release_url(),
            Duration::from_secs(config::UPDATE_TIMEOUT_SECS),
        )?;

        let tag = body
            .get("tag_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| UpdateError::Network("the release has no tag_name".into()))?;

        if !is_newer(&self.current_version, tag) {
            return Ok(None);
        }
        let version = tag.trim_start_matches(['v', 'V']).to_owned();

        let assets = body
            .get("assets")
            .and_then(|v| v.as_array())
            .ok_or_else(|| UpdateError::Network("the release has no assets".into()))?;

        let names: Vec<&str> = assets
            .iter()
            .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
            .collect();

        let expected = asset_name(config::UPDATE_ASSET_PATTERN, &version, current_target());
        let chosen =
            select_asset(names, &expected).ok_or_else(|| UpdateError::NoAssetForTarget {
                version: version.clone(),
                target: current_target().to_owned(),
            })?;

        let asset = assets
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(chosen))
            .ok_or_else(|| UpdateError::Network("the chosen asset vanished".into()))?;

        let asset_url = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| UpdateError::Network("the asset has no download url".into()))?
            .to_owned();

        Ok(Some(ReleaseInfo {
            version,
            asset_name: chosen.to_owned(),
            asset_url,
            size: asset.get("size").and_then(serde_json::Value::as_u64),
        }))
    }

    fn download_and_install(
        &self,
        release: &ReleaseInfo,
        on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(), UpdateError> {
        let staging = tempfile::Builder::new()
            .prefix("headroomlab-update-")
            .tempdir_in(staging_parent()?)
            .map_err(|e| UpdateError::Download(format!("no temporary directory: {e}")))?;

        let archive = staging.path().join(&release.asset_name);
        download(&release.asset_url, &archive, release.size, on_progress)?;

        let unpacked = staging.path().join("unpacked");
        fs::create_dir_all(&unpacked)
            .map_err(|e| UpdateError::Extract(format!("could not stage the update: {e}")))?;

        self_update::Extract::from_source(&archive)
            .extract_into(&unpacked)
            .map_err(|e| UpdateError::Extract(e.to_string()))?;

        install(&unpacked)
    }

    fn restart(&self) -> Result<std::convert::Infallible, UpdateError> {
        restart_now()
    }
}

fn staging_parent() -> Result<PathBuf, UpdateError> {
    let exe = std::env::current_exe()
        .map_err(|e| UpdateError::Unsupported(format!("cannot locate this executable: {e}")))?;

    let anchor = macos_bundle_root(&exe).unwrap_or(exe);
    anchor
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| UpdateError::Unsupported("this executable has no parent directory".into()))
}

fn macos_bundle_root(exe: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let root = exe.ancestors().nth(3)?;
    (root.extension()?.eq_ignore_ascii_case("app")).then(|| root.to_path_buf())
}

fn download(
    url: &str,
    dest: &Path,
    expected_size: Option<u64>,
    on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<(), UpdateError> {
    let mut response = client(DOWNLOAD_TIMEOUT)?
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .send()
        .map_err(|e| UpdateError::Download(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::Download(format!(
            "the download answered {}",
            response.status()
        )));
    }

    let total = expected_size.or_else(|| response.content_length());

    let file = fs::File::create(dest)
        .map_err(|e| UpdateError::Download(format!("could not write the download: {e}")))?;

    let mut counter = ProgressWriter {
        inner: io::BufWriter::new(file),
        written: 0,
        total,
        report: on_progress,
    };

    on_progress(0, total);
    io::copy(&mut response, &mut counter)
        .map_err(|e| UpdateError::Download(format!("the download stopped early: {e}")))?;

    counter
        .inner
        .flush()
        .map_err(|e| UpdateError::Download(format!("could not finish writing: {e}")))?;
    Ok(())
}

struct ProgressWriter<'a, W: Write> {
    inner: W,
    written: u64,
    total: Option<u64>,
    report: &'a (dyn Fn(u64, Option<u64>) + Send + Sync),
}

impl<W: Write> Write for ProgressWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        (self.report)(self.written, self.total);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(target_os = "macos")]
fn install(unpacked: &Path) -> Result<(), UpdateError> {
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
fn install(unpacked: &Path) -> Result<(), UpdateError> {
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

fn restart_now() -> Result<std::convert::Infallible, UpdateError> {
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

fn client(timeout: Duration) -> Result<reqwest::blocking::Client, UpdateError> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

fn get_json(url: &str, timeout: Duration) -> Result<serde_json::Value, UpdateError> {
    let response = client(timeout)?
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::Network(format!(
            "GitHub answered {}",
            response.status()
        )));
    }

    response
        .json()
        .map_err(|e| UpdateError::Network(format!("unreadable release data: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_url_points_at_the_configured_repo() {
        let updater = GitHubUpdater::new();
        assert_eq!(
            updater.latest_release_url(),
            format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                config::UPDATE_REPO_OWNER,
                config::UPDATE_REPO_NAME
            )
        );
    }

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
    fn the_counting_writer_reports_every_chunk_and_totals_correctly() {
        use std::sync::Mutex;

        let seen: Mutex<Vec<(u64, Option<u64>)>> = Mutex::new(Vec::new());
        let report = |received: u64, total: Option<u64>| {
            seen.lock().unwrap().push((received, total));
        };

        let mut sink = ProgressWriter {
            inner: Vec::new(),
            written: 0,
            total: Some(6),
            report: &report,
        };
        sink.write_all(b"abc").unwrap();
        sink.write_all(b"def").unwrap();

        assert_eq!(sink.written, 6);
        assert_eq!(sink.inner, b"abcdef");
        assert_eq!(
            *seen.lock().unwrap(),
            vec![(3, Some(6)), (6, Some(6))],
            "bytes are cumulative, not per-chunk"
        );
    }

    #[test]
    fn staging_sits_beside_the_thing_being_replaced() {
        let parent = staging_parent().expect("this test binary has a parent directory");
        let exe = std::env::current_exe().unwrap();
        let anchor = macos_bundle_root(&exe).unwrap_or(exe);
        assert_eq!(parent, anchor.parent().unwrap());
    }

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
