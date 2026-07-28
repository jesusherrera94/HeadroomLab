//! The bottom status bar, styled like VS Code's. The cursor position, language
//! and unsaved marker are live; the branch and indentation segments remain
//! mocked (they belong to later stories).

use eframe::egui::{self, RichText};

use crate::domain::text_document::{DocumentContent, language_label};
use crate::presentation::editor_controller::EditorTab;
use crate::presentation::theme;

/// The live editor facts the status bar reports.
pub struct StatusInfo<'a> {
    pub project_name: &'a str,
    pub tab: Option<&'a EditorTab>,
    /// 1-based cursor position in the active buffer, when it has focus.
    pub cursor: Option<(usize, usize)>,
}

pub fn status_bar(ui: &mut egui::Ui, info: StatusInfo<'_>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        segment(ui, &format!("⑂ main — {}", info.project_name));

        if let Some(tab) = info.tab
            && tab.unsaved()
        {
            segment(ui, &format!("● {} — unsaved", tab.name));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            segment(ui, "UTF-8");
            segment(ui, "Spaces: 2");

            let (line, col) = info.cursor.unwrap_or((1, 1));
            segment(ui, &format!("Ln {line}, Col {col}"));

            if let Some(tab) = info.tab {
                // A buffer past the highlight threshold is still editable, so say
                // so rather than leaving the user wondering why it's monochrome.
                if matches!(
                    tab.content,
                    DocumentContent::Text {
                        highlight: false,
                        ..
                    }
                ) {
                    segment(ui, "Highlighting off");
                }
                segment(ui, language_label(tab.language));
            }
        });
    });
}

fn segment(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::small_font())
            .color(theme::MUTED_ON_DARK),
    );
    ui.add_space(10.0);
}
