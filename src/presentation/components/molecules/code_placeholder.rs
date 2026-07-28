//! Read-only states of the code area: the empty state, files we refuse to load
//! into a `TextEdit` (binary / oversized), and the banner shown when an open
//! file changed on disk underneath a buffer the user has been editing.

use eframe::egui::{self, RichText};

use crate::domain::text_document::DocumentContent;
use crate::presentation::theme;

/// Centred message for a file that cannot be edited, explaining why.
pub fn code_placeholder(ui: &mut egui::Ui, content: &DocumentContent) {
    let (title, detail) = match content {
        DocumentContent::Binary => (
            "Binary file",
            "This file isn't text, so it can't be shown or edited here.".to_owned(),
        ),
        DocumentContent::TooLarge { bytes } => (
            "File too large",
            format!(
                "{} is over the {} MB editor limit.",
                human_size(*bytes),
                crate::domain::text_document::MAX_OPEN_BYTES / (1024 * 1024)
            ),
        ),
        // Editable text never reaches here; the code editor renders it.
        DocumentContent::Text { .. } => return,
    };

    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(
                RichText::new(title)
                    .font(theme::subtitle_font())
                    .color(theme::LABEL_ON_DARK),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(detail)
                    .font(theme::body_font())
                    .color(theme::MUTED_ON_DARK),
            );
        });
    });
}

/// The empty state, before any file is opened.
pub fn no_file_open(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.label(
            RichText::new("Select a file to open it")
                .font(theme::body_font())
                .color(theme::MUTED_ON_DARK),
        );
    });
}

/// Banner shown above a dirty buffer whose file changed on disk. Returns true
/// when the user asks to reload (discarding their edits).
pub fn changed_on_disk_banner(ui: &mut egui::Ui) -> bool {
    let mut reload = false;
    egui::Frame::new()
        .fill(theme::INSET_SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::ERROR_COLOR))
        .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("This file changed on disk. Your unsaved edits are kept.")
                        .font(theme::small_font())
                        .color(theme::LABEL_ON_DARK),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(theme::selectable_button("Reload from disk", false))
                        .clicked()
                    {
                        reload = true;
                    }
                });
            });
        });
    reload
}

/// Formats a byte count for the too-large message.
fn human_size(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} KB", bytes / KB)
    }
}

#[cfg(test)]
mod tests {
    use super::human_size;

    #[test]
    fn sizes_render_in_the_largest_sensible_unit() {
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human_size(1536 * 1024), "1.5 MB");
        assert_eq!(human_size(2048), "2 KB");
    }
}
