use eframe::egui::{self, CornerRadius, RichText};

use crate::domain::update::UpdateState;
use crate::presentation::components::atoms::logo;
use crate::presentation::theme;

const TRACK_WIDTH: f32 = 180.0;
const TRACK_HEIGHT: f32 = 4.0;
const SWEEP_SPEED: f32 = 0.55;
const SWEEP_WIDTH: f32 = 0.32;

#[derive(Default)]
pub struct SplashEvents {
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

            let mut status_text = egui::text::LayoutJob::default();
            status_text.append(
                &state.status_line(),
                0.0,
                egui::TextFormat {
                    font_id: theme::body_font(),
                    color: status_color(state),
                    ..Default::default()
                },
            );
            status_text.wrap.max_rows = 2;
            status_text.wrap.overflow_character = Some('…'); // default already
            ui.add(egui::Label::new(status_text).wrap());

            if let Some(readout) = state.byte_readout() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(readout)
                        .font(egui::FontId::monospace(theme::FONT_SMALL))
                        .color(theme::MUTED_ON_DARK),
                );
            }

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

fn status_color(state: &UpdateState) -> egui::Color32 {
    if state.needs_acknowledgement() {
        theme::DIAGNOSTIC_WARNING
    } else {
        theme::MUTED_ON_DARK
    }
}

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
            let phase = (ui.input(|i| i.time) as f32 * SWEEP_SPEED).rem_euclid(2.0);
            let travel = if phase <= 1.0 { phase } else { 2.0 - phase };
            let width = rect.width() * SWEEP_WIDTH;
            let x = rect.left() + (rect.width() - width) * travel;
            ui.ctx().request_repaint();
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, rect.height()))
        }
    };

    painter.rect_filled(fill, CornerRadius::same(2), theme::LOGO_STROKE);
}
