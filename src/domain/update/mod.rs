mod release;
mod state;

use std::fmt;

pub use release::{archive_extension, asset_name, current_target, is_newer, select_asset};
pub use state::UpdateState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub asset_name: String,
    pub asset_url: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    Disabled,
    Network(String),
    NoAssetForTarget { version: String, target: String },
    Download(String),
    Extract(String),
    Install(String),
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

#[cfg(test)]
mod tests {
    use super::*;

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
