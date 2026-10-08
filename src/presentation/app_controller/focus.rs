//! Which viewport is focused, which one drives the repaint tick, and raising
//! a window to the front.

use eframe::egui::{self, ViewportCommand, ViewportId};

use super::{HeadroomApp, Screen};
use crate::domain::menu::{self, WindowId};
use crate::presentation::menu_controller;

impl HeadroomApp {
    pub(super) fn ticking_viewport(&self, ctx: &egui::Context) -> ViewportId {
        let preferred = match self.screen {
            Screen::Splash => return ViewportId::ROOT,
            Screen::Initial => WindowId::Initial,
            Screen::Editor => WindowId::Editor,
        };

        let on_screen = |window: WindowId| {
            !ctx.input_for(Self::viewport_id(window), |input| {
                input.viewport().minimized.unwrap_or(false)
            })
        };

        if on_screen(preferred) {
            return Self::viewport_id(preferred);
        }
        let fallback = [
            (WindowId::Simulator, self.windows.simulator.is_some()),
            (WindowId::Graph, self.windows.graph.is_some()),
            (WindowId::Doom, self.windows.doom.is_some()),
            (WindowId::Initial, self.initial_open),
        ]
        .into_iter()
        .find(|(window, open)| *open && on_screen(*window))
        .map(|(window, _)| window);

        Self::viewport_id(fallback.unwrap_or(preferred))
    }

    pub(super) fn viewport_id(window: WindowId) -> ViewportId {
        match window {
            WindowId::Splash => ViewportId::ROOT,
            WindowId::Initial => ViewportId::from_hash_of("initial_window"),
            WindowId::Editor => ViewportId::from_hash_of("editor_window"),
            WindowId::Simulator => ViewportId::from_hash_of("simulator_window"),
            WindowId::Graph => ViewportId::from_hash_of("graph_window"),
            WindowId::Doom => ViewportId::from_hash_of("doom_window"),
        }
    }

    pub(super) fn focused_window(&self, ctx: &egui::Context) -> WindowId {
        let candidates: &[WindowId] = match self.screen {
            Screen::Splash => &[WindowId::Splash],
            Screen::Initial => &[WindowId::Initial],
            Screen::Editor => &[
                WindowId::Doom,
                WindowId::Graph,
                WindowId::Simulator,
                WindowId::Initial,
                WindowId::Editor,
            ],
        };

        let focused = candidates.iter().copied().find(|window| {
            ctx.input_for(Self::viewport_id(*window), |input| {
                input.viewport().focused.unwrap_or(false)
            })
        });

        focused.unwrap_or(match self.screen {
            Screen::Splash => WindowId::Splash,
            Screen::Initial => WindowId::Initial,
            Screen::Editor => WindowId::Editor,
        })
    }
}

pub(super) fn raise(ctx: &egui::Context, viewport_id: ViewportId) {
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
}

pub(super) fn close_window_pressed(ctx: &egui::Context) -> bool {
    let chord = menu_controller::shortcut(menu::CLOSE_TAB);
    ctx.input_mut(|input| input.consume_shortcut(&chord))
}
