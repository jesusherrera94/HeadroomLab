//! Logic for the graph window: owns its per-window state, ships recompute
//! jobs to the background worker and applies finished results. The heavy work
//! (plugin render + FFTs) never runs on the UI thread, so the simulator
//! window's controls stay responsive.

use crate::application::graph_service::{GraphData, GraphService};
use crate::application::graph_worker::GraphComputeWorker;
use crate::presentation::components::organisms::plot_grid::PaneResets;

pub struct GraphSession {
    /// Dropping the session (window closed) disconnects the job channel and
    /// cleanly shuts the worker thread down.
    worker: GraphComputeWorker,
    pub cached: Option<GraphData>,
    pub has_processed: bool,
    pub processed_status: String,
    pub resets: PaneResets,
    /// Set to bring the OS window to the front on the next frame.
    pub focus_requested: bool,
    time_full_end: f32,
    last_full_end: f32,
}

impl GraphSession {
    pub fn open(graph_service: &GraphService) -> Self {
        // The first result arrives asynchronously via `tick`.
        graph_service.mark_params_dirty();
        Self {
            worker: GraphComputeWorker::spawn(),
            cached: None,
            has_processed: false,
            processed_status: "Computing…".to_string(),
            resets: PaneResets::default(),
            focus_requested: false,
            time_full_end: 1.0,
            last_full_end: -1.0,
        }
    }

    /// Full extent of the time panes' x-axis (the track duration in seconds).
    pub fn time_full_end(&self) -> f32 {
        self.time_full_end
    }

    /// Per-frame sync, replacing the previous 100ms UI timer: applies finished
    /// worker results and submits the next job when idle and dirty.
    pub fn tick(&mut self, graph_service: &GraphService) {
        if let Some(data) = self.worker.try_recv_result() {
            self.apply_metadata(Some(&data));
            self.cached = Some(data);
        }

        // One job in flight at most: while a knob is being dragged the dirty
        // flag stays set, so the next job (with the newest control snapshot)
        // is submitted as soon as the previous result lands (latest-wins).
        if self.worker.is_idle() && graph_service.take_dirty() {
            match graph_service.build_request() {
                Some(request) => self.worker.submit(request),
                None => self.apply_metadata(None),
            }
        }
    }

    /// Updates the non-plot state (status text, full-range bounds) and resets
    /// the visible view to the full range only when the track's duration
    /// actually changes (e.g. a new file was loaded) — turning a knob must
    /// never yank the user's current zoom/pan back to the full view.
    fn apply_metadata(&mut self, data: Option<&GraphData>) {
        match data {
            Some(d) => {
                self.has_processed = d.processed.is_some();
                self.processed_status = d.processed_error.clone().unwrap_or_default();

                let full_end = d.original.duration_seconds().max(1e-3);
                self.time_full_end = full_end;
                if (full_end - self.last_full_end).abs() > 1e-6 {
                    self.last_full_end = full_end;
                    self.resets.reset_time_panes();
                }
            }
            None => {
                self.has_processed = false;
                self.processed_status = "No audio loaded yet.".to_string();
            }
        }
    }
}
