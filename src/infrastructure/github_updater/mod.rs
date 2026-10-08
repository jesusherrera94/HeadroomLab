mod http;
mod install;
mod location;

use std::fs;
use std::time::Duration;

use crate::application::ports::UpdaterPort;
use crate::config;
use crate::domain::update::{
    ReleaseInfo, UpdateError, asset_name, current_target, is_newer, select_asset,
};

use http::{download, get_json};
use install::install;
use location::{restart_now, staging_parent};

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
}
