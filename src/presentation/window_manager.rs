//! Tracks which child windows (viewports) are open and owns their per-window
//! state. Dropping a state closes the corresponding viewport on the next
//! frame; dropping the graph session also shuts its compute worker down.

use crate::application::graph_service::GraphService;
use crate::presentation::graph_controller::GraphSession;
use crate::presentation::simulation_controller::SimulatorState;

#[derive(Default)]
pub struct WindowManager {
    pub simulator: Option<SimulatorState>,
    pub graph: Option<GraphSession>,
}

impl WindowManager {
    /// Opens the simulator with a fresh initial state (matching the previous
    /// version, which created a brand-new window on every launch). If it's
    /// already open it is reset and brought back to the front.
    pub fn open_simulator(&mut self) -> &mut SimulatorState {
        let was_open = self.simulator.is_some();
        let state = self.simulator.insert(SimulatorState::default());
        state.focus_requested = was_open;
        state
    }

    /// Opens the graph window, reusing the existing session (and bringing the
    /// window back to the front) if it's already open.
    pub fn open_graph(&mut self, graph_service: &GraphService) {
        match &mut self.graph {
            Some(session) => session.focus_requested = true,
            None => self.graph = Some(GraphSession::open(graph_service)),
        }
    }

    pub fn close_simulator(&mut self) {
        self.simulator = None;
    }

    pub fn close_graph(&mut self) {
        self.graph = None;
    }

    pub fn close_all(&mut self) {
        self.simulator = None;
        self.graph = None;
    }
}
