//! Opening the hardware simulator on the project's freshly built effect library.

use std::path::PathBuf;

use super::HeadroomApp;
use crate::domain::project::{RecentProject, sanitize_target};

impl HeadroomApp {
    pub(super) fn launch_simulator(&mut self) {
        let path = self.effect_library_path();
        let state = self.windows.open_simulator();
        match self.sim_service.load_plugin(&path) {
            Ok(()) => self.graph_service.set_plugin_path(&path),
            Err(e) => {
                state.error_message = e.to_string();
                state.show_error = true;
            }
        }
    }

    pub(super) fn prepare_simulator(&mut self) {
        if self.windows.simulator.is_some() {
            return;
        }
        self.windows.open_simulator();
        self.simulator_awaiting_build = true;
    }

    pub(super) fn reveal_simulator(&mut self) {
        self.simulator_awaiting_build = false;
        let Some(state) = self.windows.simulator.as_mut() else {
            return;
        };
        state.focus_requested = true;

        let path = self.effect_library_path();
        match self.sim_service.load_plugin(&path) {
            Ok(()) => self.graph_service.set_plugin_path(&path),
            Err(e) => {
                if let Some(state) = self.windows.simulator.as_mut() {
                    state.error_message = e.to_string();
                    state.show_error = true;
                }
            }
        }
    }

    pub(super) fn discard_pending_simulator(&mut self) {
        if self.simulator_awaiting_build {
            self.simulator_awaiting_build = false;
            self.windows.close_simulator();
        }
    }

    fn effect_library_path(&self) -> String {
        self.current_project
            .as_ref()
            .and_then(effect_dylib_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

fn effect_dylib_path(project: &RecentProject) -> Option<PathBuf> {
    let target = sanitize_target(&project.name)?;
    let ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    Some(
        project
            .path
            .join("build")
            .join(format!("lib{target}.{ext}")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dylib_path_sanitizes_name_and_uses_platform_ext() {
        let project = RecentProject::new("My Fuzz", "/tmp/projects/my-fuzz");
        let ext = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        assert_eq!(
            effect_dylib_path(&project),
            Some(PathBuf::from(format!(
                "/tmp/projects/my-fuzz/build/libmy_fuzz.{ext}"
            )))
        );
    }

    #[test]
    fn dylib_path_none_when_name_has_no_valid_target() {
        let project = RecentProject::new("###", "/tmp/x");
        assert_eq!(effect_dylib_path(&project), None);
    }
}
