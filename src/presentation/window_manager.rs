//! Tracks which child windows (viewports) are open and owns their per-window
//! state. Dropping a state closes the corresponding viewport on the next
//! frame; dropping the graph session also shuts its compute worker down.

use std::rc::Rc;

use crate::application::graph_service::GraphService;
use crate::application::ports::DoomPort;
use crate::presentation::doom_controller::DoomState;
use crate::presentation::graph_controller::GraphSession;
use crate::presentation::simulation_controller::SimulatorState;

#[derive(Default)]
pub struct WindowManager {
    pub simulator: Option<SimulatorState>,
    pub graph: Option<GraphSession>,
    pub doom: Option<DoomState>,
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

    /// Opens the DOOM.666 window, reusing the running game (and bringing the
    /// window back to the front) if it's already open — clicking the file
    /// twice must not reset a run in progress.
    pub fn open_doom(&mut self, port: &Rc<dyn DoomPort>) {
        match &mut self.doom {
            Some(state) => state.focus_requested = true,
            None => self.doom = Some(DoomState::open(port.clone())),
        }
    }

    /// Closes the simulator **and the graph with it**.
    ///
    /// The graph plots the processed signal for the plugin the simulator loaded.
    /// Left open on its own it would keep showing that signal indefinitely, with
    /// nothing producing it and no way to refresh it — so the two close as one.
    /// Dropping the session also shuts its compute worker down.
    pub fn close_simulator(&mut self) {
        self.simulator = None;
        self.graph = None;
    }

    pub fn close_graph(&mut self) {
        self.graph = None;
    }

    pub fn close_doom(&mut self) {
        self.doom = None;
    }

    pub fn close_all(&mut self) {
        self.simulator = None;
        self.graph = None;
        self.doom = None;
    }
}
