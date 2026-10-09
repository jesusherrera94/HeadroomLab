//! The splash screen's update gate and acting on what the updater reports.

use eframe::egui::{self, ViewportCommand, ViewportId};

use super::{HeadroomApp, SPLASH_DURATION, Screen};
use crate::application::update_service::Trigger;
use crate::presentation::windows::splash_window;

impl HeadroomApp {
    pub(super) fn show_splash(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let events = splash_window::show(ui, self.updates.state());
        if events.continue_anyway {
            self.updates.acknowledge_failure();
        }

        let shown_long_enough = self.splash_started.elapsed() >= SPLASH_DURATION;
        if self.updates.state().is_settled() && shown_long_enough {
            // self.screen = Screen::Initial;
            // self.initial.focus_requested = true;
            // ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(false));
        }
    }

    pub(super) fn handle_update_outcome(&mut self) {
        if self.updates.trigger() == Trigger::Manual
            && self.updates.pending_release().is_some()
            && !self.updates.is_running()
        {
            let version = self
                .updates
                .pending_release()
                .map(|r| r.version.clone())
                .unwrap_or_default();
            self.update_notice = Some(format!("Downloading v{version}…"));
            self.manual_check_running = false;
            self.updates.install_found();
        }

        if let Some(version) = self.updates.take_restart_request() {
            match self.screen {
                Screen::Splash | Screen::Initial => self.restart_into_update(),
                Screen::Editor => {
                    self.pending_restart = true;
                    self.update_notice =
                        Some(format!("v{version} is installed. Restarting HeadroomLab…"));
                    self.request_quit();
                }
            }
        }

        if self.manual_check_running
            && self.updates.trigger() == Trigger::Manual
            && !self.updates.is_running()
            && self.updates.pending_release().is_none()
        {
            self.manual_check_running = false;
            if self.updates.state().is_settled() {
                self.update_notice = Some(format!(
                    "HeadroomLab v{} is the latest version.",
                    env!("CARGO_PKG_VERSION")
                ));
            }
        }

        if self.updates.state().needs_acknowledgement()
            && matches!(self.screen, Screen::Editor)
            && let Some(editor) = self.editor.as_mut()
        {
            editor.explorer.error = Some(self.updates.state().status_line());
            self.updates.acknowledge_failure();
            self.manual_check_running = false;
            self.update_notice = None;
        }
    }

    pub(super) fn restart_into_update(&mut self) {
        match self.updater.restart() {
            Ok(never) => match never {},
            Err(e) => {
                eprintln!("[updater] could not restart: {e}");
                if let Some(editor) = self.editor.as_mut() {
                    editor.explorer.error = Some(format!(
                        "The update was installed but the app could not restart: {e}\n\n                         Quit and reopen HeadroomLab to use the new version."
                    ));
                }
            }
        }
    }
}
