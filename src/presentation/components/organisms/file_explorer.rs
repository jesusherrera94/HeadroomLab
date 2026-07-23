//! The explorer side panel: an "EXPLORER" header and the (mocked) project file
//! tree. Returns the index of a clicked row, if any. The real on-demand tree
//! from disk arrives in Day 5.

use eframe::egui::{self, RichText};

use crate::presentation::components::molecules::explorer_row::explorer_row;
use crate::presentation::editor_controller::ExplorerNode;
use crate::presentation::theme;

pub fn file_explorer(ui: &mut egui::Ui, tree: &[ExplorerNode]) -> Option<usize> {
    let mut clicked = None;

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("EXPLORER")
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
        );
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, node) in tree.iter().enumerate() {
                if explorer_row(ui, node) {
                    clicked = Some(index);
                }
            }
        });

    clicked
}
