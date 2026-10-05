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
    pub fn open_simulator(&mut self) -> &mut SimulatorState {
        let was_open = self.simulator.is_some();
        let state = self.simulator.insert(SimulatorState::default());
        state.focus_requested = was_open;
        state
    }

    pub fn open_graph(&mut self, graph_service: &GraphService) {
        match &mut self.graph {
            Some(session) => session.focus_requested = true,
            None => self.graph = Some(GraphSession::open(graph_service)),
        }
    }

    pub fn open_doom(&mut self, port: &Rc<dyn DoomPort>) {
        match &mut self.doom {
            Some(state) => state.focus_requested = true,
            None => self.doom = Some(DoomState::open(port.clone())),
        }
    }

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
