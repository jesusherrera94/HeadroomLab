use std::sync::Mutex;

use super::*;
use crate::domain::update::UpdateError;

struct ScriptedUpdater {
    check: Mutex<Result<Option<ReleaseInfo>, UpdateError>>,
    install: Mutex<Result<(), UpdateError>>,
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
