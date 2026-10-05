//! Splash / updater screen: logo, title, progress bar and a status line, over
//! the app version.
//!
//! Everything below the title now reflects a real [`UpdateState`] — the bar
//! fills with the download, the status line names the version being installed,
//! and a failure offers "Continue anyways" rather than trapping the user on a
//! screen they never asked to see (AC 5).
//!
//! Layout follows `mk/design/HeadroomLab Prototype.dc.html`: a 56 px logo tile,
//! the wordmark, a 180×4 track, the status line, and the version pinned near the
//! bottom in mono.

use eframe::egui::{self, CornerRadius, RichText};

use crate::domain::update::UpdateState;
use crate::presentation::components::atoms::logo;
use crate::presentation::theme;

/// Width of the progress track, from the prototype.
const TRACK_WIDTH: f32 = 180.0;
const TRACK_HEIGHT: f32 = 4.0;
/// How far the indeterminate marker travels per second, as a fraction of the
/// track. Slow enough to read as "working", not as an animation demanding
/// attention.
const SWEEP_SPEED: f32 = 0.55;
/// How much of the track the indeterminate marker covers.
const SWEEP_WIDTH: f32 = 0.32;

#[derive(Default)]
pub struct SplashEvents {
    /// "Continue anyways" was pressed — proceed into the app despite the error.
    pub continue_anyway: bool,
}

pub fn show(ui: &mut egui::Ui, state: &UpdateState) -> SplashEvents {
    let mut events = SplashEvents::default();

    egui::CentralPanel::default().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            logo::logo_tile(ui, 56.0);
            ui.add_space(14.0);
            ui.label(
                RichText::new("Headroom Lab")
                    .font(theme::subtitle_font())
                    .color(theme::LABEL_ON_DARK),
            );
            ui.add_space(14.0);

            progress_bar(ui, state.progress());
            ui.add_space(8.0);

            ui.label(
                RichText::new(state.status_line())
                    .font(theme::body_font())
                    .color(status_color(state)),
            );

            // Byte counts sit under the status line so the line above can stay
            // still while the numbers move.
            if let Some(readout) = state.byte_readout() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(readout)
                        .font(egui::FontId::monospace(theme::FONT_SMALL))
                        .color(theme::MUTED_ON_DARK),
                );
            }

            // The escape hatch, and the only interactive thing on this screen.
            // Present only on failure (D4) — an update that is going well is not
            // something the user should have to dismiss.
            if state.needs_acknowledgement() {
                ui.add_space(10.0);
                let button = egui::Button::new(
                    RichText::new("Continue anyways")
                        .font(theme::body_font())
                        .color(theme::LOGO_STROKE),
                )
                .frame(false);
                if ui.add(button).clicked() {
                    events.continue_anyway = true;
                }
            }

            ui.add_space(24.0);
            ui.label(
                RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                    .font(egui::FontId::monospace(theme::FONT_SMALL))
                    .color(theme::MUTED_ON_DARK),
            );
        });
    });

    events
}

/// A failure is the one thing on this screen worth colouring.
fn status_color(state: &UpdateState) -> egui::Color32 {
    if state.needs_acknowledgement() {
        theme::DIAGNOSTIC_WARNING
    } else {
        theme::MUTED_ON_DARK
    }
}

/// The progress track.
///
/// `Some(fraction)` fills from the left. `None` sweeps a marker back and forth,
/// which is what a check — or a download whose size the server never declared —
/// honestly looks like: something is happening, and we cannot say how much is
/// left.
fn progress_bar(ui: &mut egui::Ui, fraction: Option<f32>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(TRACK_WIDTH, TRACK_HEIGHT), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), theme::INSET_SURFACE);

    let fill = match fraction {
        Some(fraction) => {
            let mut fill = rect;
            fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
            fill
        }
        None => {
            // Ping-pong on a triangle wave, so the marker turns around at the
            // ends instead of jumping back to the start.
            let phase = (ui.input(|i| i.time) as f32 * SWEEP_SPEED).rem_euclid(2.0);
            let travel = if phase <= 1.0 { phase } else { 2.0 - phase };
            let width = rect.width() * SWEEP_WIDTH;
            let x = rect.left() + (rect.width() - width) * travel;
            // The sweep never settles, so keep asking for frames.
            ui.ctx().request_repaint();
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, rect.height()))
        }
    };

    painter.rect_filled(fill, CornerRadius::same(2), theme::LOGO_STROKE);
}
