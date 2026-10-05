//! Runs the updater off the UI thread.
//!
//! Same shape as `graph_worker`: a named thread, a job channel, and a
//! non-blocking poll the UI drains from its existing repaint tick. The
//! difference is what flows back — an update reports *while* it works, so the
//! return channel carries a stream of [`UpdateEvent`]s rather than one result.
//!
//! Only one job ever runs at a time. The port's calls block for as long as the
//! network takes, and a second concurrent install would be racing the first for
//! the same files.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

use crate::application::ports::UpdaterPort;
use crate::domain::update::{ReleaseInfo, UpdateError};

/// What the worker is being asked to do.
///
/// Each job is atomic and ends in exactly one terminal event. Checking and
/// installing are deliberately *separate* jobs rather than one combined one:
/// only the service knows whether a found release should be installed at once
/// (the splash) or held until the unsaved-work guard has been answered (D7), and
/// a single job that did both would have to clear its busy flag halfway through
/// to report the find — letting a second install be submitted on top of the one
/// still running.
pub enum UpdateJob {
    /// Look for a newer release. Ends in `UpToDate` or `Found`.
    Check,
    /// Install a release a `Check` turned up.
    Install(ReleaseInfo),
}

/// What the worker reports back. A job produces zero or more `Progress` events
/// and exactly one terminal event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    /// A check found nothing to do — or failed, which the service treats the
    /// same way (D5).
    UpToDate,
    /// A check found a release. The service decides what happens next.
    Found(ReleaseInfo),
    Progress {
        version: String,
        received: u64,
        total: Option<u64>,
    },
    /// The download finished and the swap is under way.
    Installing {
        version: String,
    },
    /// The new build is in place and the app may hand over to it.
    Installed {
        version: String,
    },
    Failed(UpdateError),
}

/// Owns the update thread. Dropping it closes the job channel, which ends the
/// thread once its current job returns.
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

    /// Queues a job, unless one is already running. Returns whether it was taken.
    pub fn submit(&mut self, job: UpdateJob) -> bool {
        if self.busy {
            return false;
        }
        if self.job_tx.send(job).is_err() {
            // The thread is gone; nothing will ever answer.
            return false;
        }
        self.busy = true;
        true
    }

    /// Non-blocking poll. Clears the busy flag on a terminal event, so the next
    /// job can be submitted.
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

/// Whether this event ends the job that produced it. One rule, because every
/// job is atomic — see [`UpdateJob`].
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
            // Reported as-is, including failures: whether a failed check is
            // worth showing depends on *why* it ran, and only the service knows
            // that (D5). Deciding here would tell a user who explicitly asked
            // "am I up to date?" that they are, when in truth we never found out.
            UpdateJob::Check => match updater.check() {
                Ok(Some(release)) => UpdateEvent::Found(release),
                Ok(None) => UpdateEvent::UpToDate,
                Err(e) => UpdateEvent::Failed(e),
            },
            UpdateJob::Install(release) => install(updater, &release, events),
        };

        if events.send(outcome).is_err() {
            // The UI has gone; so should we.
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
        // A closed channel means the UI is gone. Nothing useful to do about it
        // here; the send after this call returns will end the loop.
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

    /// A port that answers from a script, with no network anywhere.
    struct FakeUpdater {
        check_result: Mutex<Option<Result<Option<ReleaseInfo>, UpdateError>>>,
        install_result: Mutex<Option<Result<(), UpdateError>>>,
        /// Byte pairs the install reports before returning.
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

    /// Drains events until a terminal one arrives, or the deadline passes.
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

    /// AC 5: a failed *download* does surface, unlike a failed check.
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

    /// A check never installs on its own — that is the service's call (D7).
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
