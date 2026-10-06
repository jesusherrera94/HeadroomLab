use std::path::PathBuf;

use directories::ProjectDirs;

use crate::application::ports::{RecentProjectsError, RecentProjectsStore};
use crate::domain::project::RecentProject;

const FILE_NAME: &str = "recent_projects.json";

pub struct FileRecentProjectsStore {
    file: Option<PathBuf>,
}

impl FileRecentProjectsStore {
    pub fn new() -> Self {
        let file = ProjectDirs::from("com", "HeadroomLab", "HeadroomLab")
            .map(|dirs| dirs.config_dir().join(FILE_NAME));
        Self { file }
    }
}

impl Default for FileRecentProjectsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RecentProjectsStore for FileRecentProjectsStore {
    fn load(&self) -> Vec<RecentProject> {
        let Some(file) = &self.file else {
            return Vec::new();
        };
        let Ok(contents) = std::fs::read_to_string(file) else {
            return Vec::new();
        };
        serde_json::from_str(&contents).unwrap_or_default()
    }

    fn save(&self, items: &[RecentProject]) -> Result<(), RecentProjectsError> {
        let Some(file) = &self.file else {
            return Err(RecentProjectsError("no config directory available".into()));
        };
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| RecentProjectsError(e.to_string()))?;
        }
        let json =
            serde_json::to_string_pretty(items).map_err(|e| RecentProjectsError(e.to_string()))?;
        std::fs::write(file, json).map_err(|e| RecentProjectsError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_at(path: PathBuf) -> FileRecentProjectsStore {
        FileRecentProjectsStore { file: Some(path) }
    }

    fn temp_file(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("hl_recents_test_{tag}_{}", std::process::id()));
        dir.push(FILE_NAME);
        dir
    }

    #[test]
    fn round_trips_through_json() {
        let path = temp_file("roundtrip");
        let store = store_at(path.clone());
        let items = vec![
            RecentProject::new("Alpha", "/tmp/alpha"),
            RecentProject::new("Beta", "/tmp/beta"),
        ];
        store.save(&items).unwrap();
        assert_eq!(store.load(), items);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_file_loads_empty() {
        let store = store_at(temp_file("missing"));
        assert!(store.load().is_empty());
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let path = temp_file("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json ]").unwrap();
        let store = store_at(path.clone());
        assert!(store.load().is_empty());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
