//! Owns the update flow's state and turns worker events into it.
//!
//! The service is the only thing that knows *why* an update is running, and
//! that is the whole reason it exists rather than the UI reading the worker
//! directly. The two callers want different endings:
//!
//! * the **splash** installs and restarts straight away (D4);
//! * a **manual check** from the Editor must stop short of restarting until the
//!   unsaved-work guard has been answered (D7).
//!
//! So `Trigger` is carried alongside the state, and `take_restart_request` is
//! what the app controller polls to know an install is ready to hand over.

use std::sync::Arc;

use crate::application::ports::UpdaterPort;
use crate::application::update_worker::{UpdateEvent, UpdateJob, UpdateWorker};
use crate::config;
use crate::domain::update::{ReleaseInfo, UpdateState};

/// Why an update run was started. Decides what happens once it succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The splash at launch: install and restart without asking.
    Startup,
    /// Someone chose "Check for Updates…": tell them the outcome either way,
    /// and route the restart through the quit guard.
    Manual,
}

pub struct UpdateService {
    /// Kept so the composition root does not have to hold a second handle: the
    /// app controller needs it to hand over after an install.
    updater: Arc<dyn UpdaterPort>,
    worker: UpdateWorker,
    state: UpdateState,
    trigger: Trigger,
    /// Set once an install has landed; taken by the app controller, which owns
    /// the decision about when it is safe to restart.
    restart_pending: Option<String>,
    /// Set when a manual check found something, so the caller can say so.
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

    /// The port itself, for the caller that has to perform the handover.
    pub fn updater(&self) -> Arc<dyn UpdaterPort> {
        self.updater.clone()
    }

    pub fn trigger(&self) -> Trigger {
        self.trigger
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_busy()
    }

    /// Whether the updater is switched on for this build (D3).
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Starts the launch-time check.
    ///
    /// With updates disabled this settles on `UpToDate` immediately, so the
    /// splash's advance condition is the same either way and a debug build never
    /// touches the network.
    pub fn start_at_launch(&mut self) {
        if !self.enabled {
            self.state = UpdateState::UpToDate;
            return;
        }
        self.trigger = Trigger::Startup;
        self.state = UpdateState::Checking;
        self.worker.submit(UpdateJob::Check);
    }

    /// Starts a manual check. Ignored while a run is already going.
    pub fn check_manually(&mut self) {
        if !self.enabled || self.worker.is_busy() {
            return;
        }
        self.trigger = Trigger::Manual;
        self.state = UpdateState::Checking;
        self.found = None;
        self.worker.submit(UpdateJob::Check);
    }

    /// Installs the release a manual check turned up.
    pub fn install_found(&mut self) {
        let Some(release) = self.found.take() else {
            return;
        };
        self.state = UpdateState::Installing {
            version: release.version.clone(),
        };
        self.worker.submit(UpdateJob::Install(release));
    }

    /// Drains the worker. Call once per frame.
    pub fn tick(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            self.apply(event);
        }
    }

    fn apply(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::UpToDate => self.state = UpdateState::UpToDate,

            // Checking and installing are separate worker jobs, so the find
            // lands here and the service decides what follows — which is the
            // whole difference between the two triggers.
            UpdateEvent::Found(release) => match self.trigger {
                Trigger::Startup => {
                    self.state = UpdateState::Downloading {
                        version: release.version.clone(),
                        received: 0,
                        total: release.size,
                    };
                    self.worker.submit(UpdateJob::Install(release));
                }
                // A manual check stops here and waits to be told to install, so
                // the restart can be put behind the unsaved-work guard (D7).
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

            // Where D5 actually lives. A launch-time check the user never asked
            // for must not put a network error in front of them, so it settles
            // as "up to date" and the reason goes to stderr. Everything else —
            // a failed download, or any failure during a check they *did* ask
            // for — is shown.
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

    /// The release a manual check found and is waiting to install, if any.
    pub fn pending_release(&self) -> Option<&ReleaseInfo> {
        self.found.as_ref()
    }

    /// Takes the "an install is in place, hand over to it" signal.
    ///
    /// Returned rather than acted on because restarting is the app controller's
    /// call: from the splash it happens at once, but from the Editor it has to
    /// wait for the unsaved-work guard (D7).
    pub fn take_restart_request(&mut self) -> Option<String> {
        self.restart_pending.take()
    }

    /// Dismisses a failure ("Continue anyways"), returning the flow to rest.
    pub fn acknowledge_failure(&mut self) {
        if self.state.needs_acknowledgement() {
            self.state = UpdateState::UpToDate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::update::UpdateError;
    use std::sync::Mutex;

    struct ScriptedUpdater {
        check: Mutex<Result<Option<ReleaseInfo>, UpdateError>>,
        install: Mutex<Result<(), UpdateError>>,
        /// How long the download dawdles after reporting progress. Zero for most
        /// tests; a real interval where the point is to *observe* the download,
        /// since `tick` drains the whole queue and an instant install would
        /// collapse Progress and Installed into one frame.
        linger: std::time::Duration,
    }

    impl ScriptedUpdater {
        fn up_to_date() -> Arc<Self> {
            Arc::new(Self {
                check: Mutex::new(Ok(None)),
                install: Mutex::new(Ok(())),
                linger: std::time::Duration::ZERO,
            })
        }
        fn finds(release: ReleaseInfo) -> Arc<Self> {
            Arc::new(Self {
                check: Mutex::new(Ok(Some(release))),
                install: Mutex::new(Ok(())),
                linger: std::time::Duration::ZERO,
            })
        }
        fn finds_slowly(release: ReleaseInfo) -> Arc<Self> {
            Arc::new(Self {
                check: Mutex::new(Ok(Some(release))),
                install: Mutex::new(Ok(())),
                linger: std::time::Duration::from_millis(150),
            })
        }
    }

    impl UpdaterPort for ScriptedUpdater {
        fn check(&self) -> Result<Option<ReleaseInfo>, UpdateError> {
            self.check.lock().unwrap().clone()
        }
        fn download_and_install(
            &self,
            release: &ReleaseInfo,
            on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
        ) -> Result<(), UpdateError> {
            on_progress(release.size.unwrap_or(0) / 2, release.size);
            std::thread::sleep(self.linger);
            self.install.lock().unwrap().clone()
        }
        fn restart(&self) -> Result<std::convert::Infallible, UpdateError> {
            Err(UpdateError::Unsupported("not in a test".into()))
        }
    }

    fn release() -> ReleaseInfo {
        ReleaseInfo {
            version: "9.9.9".into(),
            asset_name: "a.tar.gz".into(),
            asset_url: "https://example.invalid/a".into(),
            size: Some(200),
        }
    }

    /// Pumps `tick` until the flow stops moving, as the frame loop would.
    fn settle(service: &mut UpdateService) {
        for _ in 0..400 {
            service.tick();
            if !service.is_running() {
                service.tick();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("update never settled: {:?}", service.state());
    }

    fn enabled(updater: Arc<dyn UpdaterPort>) -> UpdateService {
        let mut service = UpdateService::new(updater);
        // The config default is off in debug; these tests are about the flow.
        service.enabled = true;
        service
    }

    #[test]
    fn a_disabled_build_settles_immediately_without_touching_the_port() {
        let mut service = UpdateService::new(ScriptedUpdater::up_to_date());
        service.enabled = false;
        service.start_at_launch();

        assert_eq!(*service.state(), UpdateState::UpToDate);
        assert!(!service.is_running(), "no job was ever submitted");
        assert!(service.state().is_settled(), "the splash advances anyway");
    }

    #[test]
    fn an_up_to_date_launch_settles() {
        let mut service = enabled(ScriptedUpdater::up_to_date());
        service.start_at_launch();
        assert_eq!(*service.state(), UpdateState::Checking);

        settle(&mut service);
        assert_eq!(*service.state(), UpdateState::UpToDate);
        assert!(service.take_restart_request().is_none());
    }

    /// D5: a launch-time check that fails settles silently.
    #[test]
    fn a_failed_check_settles_silently_rather_than_failing() {
        let updater = ScriptedUpdater::up_to_date();
        *updater.check.lock().unwrap() = Err(UpdateError::Network("offline".into()));
        let mut service = enabled(updater);

        service.start_at_launch();
        settle(&mut service);

        assert_eq!(*service.state(), UpdateState::UpToDate);
        assert!(!service.state().needs_acknowledgement());
    }

    #[test]
    fn a_launch_update_runs_through_to_a_restart_request() {
        let mut service = enabled(ScriptedUpdater::finds(release()));
        service.start_at_launch();
        settle(&mut service);

        assert_eq!(
            *service.state(),
            UpdateState::Restarting {
                version: "9.9.9".into()
            }
        );
        assert_eq!(service.take_restart_request().as_deref(), Some("9.9.9"));
        assert!(
            service.take_restart_request().is_none(),
            "taking it consumes it"
        );
    }

    /// AC 5: the download failure the user does see.
    #[test]
    fn a_failed_install_waits_for_continue_anyways() {
        let updater = ScriptedUpdater::finds(release());
        *updater.install.lock().unwrap() = Err(UpdateError::Download("reset".into()));
        let mut service = enabled(updater);

        service.start_at_launch();
        settle(&mut service);

        assert!(service.state().needs_acknowledgement());
        assert!(!service.state().is_settled(), "it must not advance itself");
        assert!(service.take_restart_request().is_none());

        service.acknowledge_failure();
        assert!(service.state().is_settled(), "and now it may");
    }

    /// D7: a manual check reports but installs nothing until told.
    #[test]
    fn a_manual_check_stops_at_found() {
        let mut service = enabled(ScriptedUpdater::finds(release()));
        service.check_manually();
        settle(&mut service);

        assert_eq!(service.trigger(), Trigger::Manual);
        assert_eq!(
            service.pending_release().map(|r| r.version.as_str()),
            Some("9.9.9")
        );
        assert!(
            service.take_restart_request().is_none(),
            "nothing was installed"
        );

        service.install_found();
        settle(&mut service);
        assert_eq!(service.take_restart_request().as_deref(), Some("9.9.9"));
    }

    /// The other half of D5: a check the user *asked* for reports its failure,
    /// because "up to date" would be a claim we never verified.
    #[test]
    fn a_failed_manual_check_is_shown() {
        let updater = ScriptedUpdater::up_to_date();
        *updater.check.lock().unwrap() = Err(UpdateError::NoAssetForTarget {
            version: "1.2.0".into(),
            target: "linux-aarch64".into(),
        });
        let mut service = enabled(updater);

        service.check_manually();
        settle(&mut service);

        assert!(service.state().needs_acknowledgement());
        assert!(service.state().status_line().contains("linux-aarch64"));
    }

    /// A failed *download* is shown whatever started it (AC 5).
    #[test]
    fn a_failed_download_is_shown_even_at_launch() {
        let updater = ScriptedUpdater::finds(release());
        *updater.install.lock().unwrap() = Err(UpdateError::Download("reset".into()));
        let mut service = enabled(updater);

        service.start_at_launch();
        settle(&mut service);

        assert!(
            service.state().needs_acknowledgement(),
            "only the *check* is silent, never the download"
        );
    }

    #[test]
    fn a_manual_check_on_a_current_build_finds_nothing() {
        let mut service = enabled(ScriptedUpdater::up_to_date());
        service.check_manually();
        settle(&mut service);

        assert_eq!(*service.state(), UpdateState::UpToDate);
        assert!(service.pending_release().is_none());
    }

    #[test]
    fn a_disabled_build_ignores_a_manual_check() {
        let mut service = UpdateService::new(ScriptedUpdater::finds(release()));
        service.enabled = false;
        service.check_manually();

        assert_eq!(*service.state(), UpdateState::Idle);
        assert!(!service.is_running());
    }

    /// AC 2: byte counts have to reach the state the splash renders.
    ///
    /// The fake lingers mid-download on purpose. `tick` drains every queued
    /// event, so an install that finishes instantly would apply Progress and
    /// Installed in the same frame and the bar would never be seen to move —
    /// true of the test, not of a real 30 MB download.
    #[test]
    fn progress_reaches_the_state() {
        let mut service = enabled(ScriptedUpdater::finds_slowly(release()));
        service.start_at_launch();

        let mut seen: Option<(u64, Option<u64>)> = None;
        for _ in 0..400 {
            service.tick();
            if let UpdateState::Downloading {
                received, total, ..
            } = service.state()
                && *received > 0
            {
                seen = Some((*received, *total));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        assert_eq!(seen, Some((100, Some(200))), "the bar never moved");
        settle(&mut service);
        assert_eq!(service.take_restart_request().as_deref(), Some("9.9.9"));
    }
}
