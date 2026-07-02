use std::cell::Cell;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;

use crate::application::graph_service::{compute_graph_data, GraphComputeRequest, GraphData};

/// Runs graph recomputes on a dedicated background thread so the Slint event
/// loop (and therefore the simulator controls) never blocks on plugin renders
/// or FFTs.
///
/// The owner submits a job only while `is_idle()` and polls for the result,
/// giving latest-wins behaviour: while a knob is being dragged the next job
/// simply carries the newest control snapshot. Dropping the worker closes the
/// job channel, which cleanly shuts the thread down.
pub struct GraphComputeWorker {
    job_tx: Sender<GraphComputeRequest>,
    result_rx: Receiver<GraphData>,
    in_flight: Cell<usize>,
}

impl GraphComputeWorker {
    pub fn spawn() -> Self {
        let (job_tx, job_rx) = channel::<GraphComputeRequest>();
        let (result_tx, result_rx) = channel::<GraphData>();

        thread::Builder::new()
            .name("graph-compute".into())
            .spawn(move || worker_loop(job_rx, result_tx))
            .expect("failed to spawn graph compute thread");

        Self {
            job_tx,
            result_rx,
            in_flight: Cell::new(0),
        }
    }

    /// Queues a recompute. The result arrives later via `try_recv_result`.
    pub fn submit(&self, request: GraphComputeRequest) {
        if self.job_tx.send(request).is_ok() {
            self.in_flight.set(self.in_flight.get() + 1);
        }
    }

    /// Non-blocking poll for a finished recompute.
    pub fn try_recv_result(&self) -> Option<GraphData> {
        match self.result_rx.try_recv() {
            Ok(data) => {
                self.in_flight.set(self.in_flight.get().saturating_sub(1));
                Some(data)
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.in_flight.get() == 0
    }
}

fn worker_loop(job_rx: Receiver<GraphComputeRequest>, result_tx: Sender<GraphData>) {
    // Original waveform/spectrum cache, keyed by the request's audio generation:
    // knob changes never alter the original signal, so its FFT is reused until
    // a new audio file is loaded.
    let mut cached_original: Option<(u64, crate::domain::signal::Waveform, crate::domain::signal::Spectrum)> = None;

    while let Ok(request) = job_rx.recv() {
        let reusable = cached_original
            .as_ref()
            .filter(|(generation, _, _)| *generation == request.audio_generation)
            .map(|(_, wave, spectrum)| (wave.clone(), spectrum.clone()));

        let data = compute_graph_data(&request, reusable);
        cached_original = Some((
            request.audio_generation,
            data.original.clone(),
            data.original_spectrum.clone(),
        ));

        if result_tx.send(data).is_err() {
            return; // owner dropped: shut down
        }
    }
}
