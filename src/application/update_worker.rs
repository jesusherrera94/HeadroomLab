use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

use crate::application::ports::UpdaterPort;
use crate::domain::update::{ReleaseInfo, UpdateError};

pub enum UpdateJob {
    Check,
    Install(ReleaseInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    UpToDate,
    Found(ReleaseInfo),
    Progress {
        version: String,
        received: u64,
        total: Option<u64>,
    },
    Installing {
        version: String,
    },
    Installed {
        version: String,
    },
    Failed(UpdateError),
}

pub struct UpdateWorker {
    job_tx: Sender<UpdateJob>,
    event_rx: Receiver<UpdateEvent>,
    busy: bool,
}

impl UpdateWorker {
    pub fn spawn(updater: Arc<dyn UpdaterPort>) -> Self {
        let (job_tx, job_rx) = channel::<UpdateJob>();
        let (event_tx, event_rx) = channel::<UpdateEvent>();

        thread::Builder::new()
            .name("updater".into())
            .spawn(move || worker_loop(&job_rx, &event_tx, updater.as_ref()))
            .expect("failed to spawn updater thread");

        Self {
            job_tx,
            event_rx,
            busy: false,
        }
    }

    pub fn submit(&mut self, job: UpdateJob) -> bool {
        if self.busy {
            return false;
        }
        if self.job_tx.send(job).is_err() {
            return false;
        }
        self.busy = true;
        true
    }

    pub fn try_recv(&mut self) -> Option<UpdateEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => {
                if is_terminal(&event) {
                    self.busy = false;
                }
                Some(event)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.busy = false;
                None
            }
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }
}

fn is_terminal(event: &UpdateEvent) -> bool {
    matches!(
        event,
        UpdateEvent::UpToDate
            | UpdateEvent::Found(_)
            | UpdateEvent::Installed { .. }
            | UpdateEvent::Failed(_)
    )
}

fn worker_loop(
    jobs: &Receiver<UpdateJob>,
    events: &Sender<UpdateEvent>,
    updater: &dyn UpdaterPort,
) {
    while let Ok(job) = jobs.recv() {
        let outcome = match job {
            UpdateJob::Check => match updater.check() {
                Ok(Some(release)) => UpdateEvent::Found(release),
                Ok(None) => UpdateEvent::UpToDate,
                Err(e) => UpdateEvent::Failed(e),
            },
            UpdateJob::Install(release) => install(updater, &release, events),
        };

        if events.send(outcome).is_err() {
            return;
        }
    }
}

fn install(
    updater: &dyn UpdaterPort,
    release: &ReleaseInfo,
    events: &Sender<UpdateEvent>,
) -> UpdateEvent {
    let version = release.version.clone();

    let progress_tx = events.clone();
    let progress_version = version.clone();
    let on_progress = move |received: u64, total: Option<u64>| {
        let _ = progress_tx.send(UpdateEvent::Progress {
            version: progress_version.clone(),
            received,
            total,
        });
    };

    if events
        .send(UpdateEvent::Installing {
            version: version.clone(),
        })
        .is_err()
    {
        return UpdateEvent::Failed(UpdateError::Install("the app is shutting down".into()));
    }

    match updater.download_and_install(release, &on_progress) {
        Ok(()) => UpdateEvent::Installed { version },
        Err(e) => UpdateEvent::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    struct FakeUpdater {
        check_result: Mutex<Option<Result<Option<ReleaseInfo>, UpdateError>>>,
        install_result: Mutex<Option<Result<(), UpdateError>>>,
        progress: Vec<(u64, Option<u64>)>,
        checks: AtomicUsize,
        installs: AtomicUsize,
    }

    impl FakeUpdater {
        fn new() -> Self {
            Self {
                check_result: Mutex::new(Some(Ok(None))),
                install_result: Mutex::new(Some(Ok(()))),
                progress: Vec::new(),
                checks: AtomicUsize::new(0),
                installs: AtomicUsize::new(0),
            }
        }

        fn finding(release: ReleaseInfo) -> Self {
            let fake = Self::new();
            *fake.check_result.lock().unwrap() = Some(Ok(Some(release)));
            fake
        }
    }

    impl UpdaterPort for FakeUpdater {
        fn check(&self) -> Result<Option<ReleaseInfo>, UpdateError> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            self.check_result
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(Ok(None))
        }

        fn download_and_install(
            &self,
            _release: &ReleaseInfo,
            on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
        ) -> Result<(), UpdateError> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            for (received, total) in &self.progress {
                on_progress(*received, *total);
            }
            self.install_result
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(Ok(()))
        }

        fn restart(&self) -> Result<std::convert::Infallible, UpdateError> {
            Err(UpdateError::Unsupported("not in a test".into()))
        }
    }

    fn release() -> ReleaseInfo {
        ReleaseInfo {
            version: "1.2.0".into(),
            asset_name: "HeadroomLab-1.2.0-test.tar.gz".into(),
            asset_url: "https://example.invalid/a".into(),
            size: Some(100),
        }
    }

    fn drain(worker: &mut UpdateWorker) -> Vec<UpdateEvent> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while Instant::now() < deadline {
            match worker.try_recv() {
                Some(event) => {
                    let done = is_terminal(&event);
                    events.push(event);
                    if done {
                        return events;
                    }
                }
                None => thread::sleep(Duration::from_millis(5)),
            }
        }
        panic!("worker produced no terminal event: {events:?}");
    }

    #[test]
    fn an_up_to_date_check_reports_once_and_frees_the_worker() {
        let mut worker = UpdateWorker::spawn(Arc::new(FakeUpdater::new()));
        assert!(worker.submit(UpdateJob::Check));

        assert_eq!(drain(&mut worker), vec![UpdateEvent::UpToDate]);
        assert!(!worker.is_busy(), "a terminal event frees the worker");
    }

    /// The worker reports the failure verbatim; folding it into "up to date"
    /// is the service's call, because it depends on the trigger (D5).
    #[test]
    fn a_failed_check_reports_the_error_rather_than_deciding() {
        let fake = FakeUpdater::new();
        *fake.check_result.lock().unwrap() = Some(Err(UpdateError::Network("offline".into())));
        let mut worker = UpdateWorker::spawn(Arc::new(fake));

        worker.submit(UpdateJob::Check);
        assert_eq!(
            drain(&mut worker),
            vec![UpdateEvent::Failed(UpdateError::Network("offline".into()))]
        );
    }

    #[test]
    fn a_check_that_finds_something_ends_there() {
        let mut worker = UpdateWorker::spawn(Arc::new(FakeUpdater::finding(release())));
        worker.submit(UpdateJob::Check);
        assert_eq!(drain(&mut worker), vec![UpdateEvent::Found(release())]);
    }

    #[test]
    fn an_install_reports_progress_then_lands() {
        let mut fake = FakeUpdater::finding(release());
        fake.progress = vec![(50, Some(100)), (100, Some(100))];
        let mut worker = UpdateWorker::spawn(Arc::new(fake));

        worker.submit(UpdateJob::Install(release()));
        let events = drain(&mut worker);

        assert_eq!(
            events.first(),
            Some(&UpdateEvent::Installing {
                version: "1.2.0".into()
            })
        );
        assert!(events.contains(&UpdateEvent::Progress {
            version: "1.2.0".into(),
            received: 50,
            total: Some(100),
        }));
        assert_eq!(
            events.last(),
            Some(&UpdateEvent::Installed {
                version: "1.2.0".into()
            })
        );
    }

    #[test]
    fn a_failed_install_reports_the_error() {
        let fake = FakeUpdater::finding(release());
        *fake.install_result.lock().unwrap() =
            Some(Err(UpdateError::Download("connection reset".into())));
        let mut worker = UpdateWorker::spawn(Arc::new(fake));

        worker.submit(UpdateJob::Install(release()));
        let events = drain(&mut worker);

        assert_eq!(
            events.last(),
            Some(&UpdateEvent::Failed(UpdateError::Download(
                "connection reset".into()
            )))
        );
    }

    #[test]
    fn a_check_never_installs_by_itself() {
        let fake = Arc::new(FakeUpdater::finding(release()));
        let mut worker = UpdateWorker::spawn(fake.clone());

        worker.submit(UpdateJob::Check);
        drain(&mut worker);
        assert_eq!(fake.installs.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_second_job_is_refused_while_one_is_running() {
        let fake = Arc::new(FakeUpdater::finding(release()));
        let mut worker = UpdateWorker::spawn(fake.clone());

        assert!(worker.submit(UpdateJob::Check));
        assert!(
            !worker.submit(UpdateJob::Check),
            "two installs would race for the same files"
        );

        drain(&mut worker);
        assert!(worker.submit(UpdateJob::Check), "and freed afterwards");
        drain(&mut worker);
        assert_eq!(fake.checks.load(Ordering::SeqCst), 2);
    }
}
