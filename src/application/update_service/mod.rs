use std::sync::Arc;

use crate::application::ports::UpdaterPort;
use crate::application::update_worker::{UpdateEvent, UpdateJob, UpdateWorker};
use crate::config;
use crate::domain::update::{ReleaseInfo, UpdateState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Startup,
    Manual,
}

pub struct UpdateService {
    updater: Arc<dyn UpdaterPort>,
    worker: UpdateWorker,
    state: UpdateState,
    trigger: Trigger,
    restart_pending: Option<String>,
    found: Option<ReleaseInfo>,
    enabled: bool,
}

impl UpdateService {
    pub fn new(updater: Arc<dyn UpdaterPort>) -> Self {
        Self {
            updater: updater.clone(),
            worker: UpdateWorker::spawn(updater),
            state: UpdateState::Idle,
            trigger: Trigger::Startup,
            restart_pending: None,
            found: None,
            enabled: config::UPDATE_ENABLED,
        }
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn updater(&self) -> Arc<dyn UpdaterPort> {
        self.updater.clone()
    }

    pub fn trigger(&self) -> Trigger {
        self.trigger
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_busy()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn start_at_launch(&mut self) {
        if !self.enabled {
            // self.state = UpdateState::UpToDate;
            self.state = UpdateState::Failed {
                message: "Download failed: connection reset with very very very very very very very very very very very very very very very very very very very very very very  long description".into(),
            };
            return;
        }
        self.trigger = Trigger::Startup;
        self.state = UpdateState::Checking;
        self.worker.submit(UpdateJob::Check);
    }

    pub fn check_manually(&mut self) {
        if !self.enabled || self.worker.is_busy() {
            return;
        }
        self.trigger = Trigger::Manual;
        self.state = UpdateState::Checking;
        self.found = None;
        self.worker.submit(UpdateJob::Check);
    }

    pub fn install_found(&mut self) {
        let Some(release) = self.found.take() else {
            return;
        };
        self.state = UpdateState::Installing {
            version: release.version.clone(),
        };
        self.worker.submit(UpdateJob::Install(release));
    }

    pub fn tick(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            self.apply(event);
        }
    }

    fn apply(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::UpToDate => self.state = UpdateState::UpToDate,

            UpdateEvent::Found(release) => match self.trigger {
                Trigger::Startup => {
                    self.state = UpdateState::Downloading {
                        version: release.version.clone(),
                        received: 0,
                        total: release.size,
                    };
                    self.worker.submit(UpdateJob::Install(release));
                }
                Trigger::Manual => {
                    self.state = UpdateState::Idle;
                    self.found = Some(release);
                }
            },

            UpdateEvent::Progress {
                version,
                received,
                total,
            } => {
                self.state = UpdateState::Downloading {
                    version,
                    received,
                    total,
                };
            }

            UpdateEvent::Installing { version } => {
                self.state = UpdateState::Installing { version };
            }

            UpdateEvent::Installed { version } => {
                self.state = UpdateState::Restarting {
                    version: version.clone(),
                };
                self.restart_pending = Some(version);
            }

            UpdateEvent::Failed(error) => {
                let silent =
                    self.trigger == Trigger::Startup && matches!(self.state, UpdateState::Checking);
                if silent {
                    eprintln!("[updater] check failed, continuing without it: {error}");
                    self.state = UpdateState::from_check_error();
                } else {
                    self.state = UpdateState::Failed {
                        message: error.to_string(),
                    };
                }
            }
        }
    }

    pub fn pending_release(&self) -> Option<&ReleaseInfo> {
        self.found.as_ref()
    }

    pub fn take_restart_request(&mut self) -> Option<String> {
        self.restart_pending.take()
    }

    pub fn acknowledge_failure(&mut self) {
        if self.state.needs_acknowledgement() {
            self.state = UpdateState::UpToDate;
        }
    }
}

#[cfg(test)]
mod tests;
