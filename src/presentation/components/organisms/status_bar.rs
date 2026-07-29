//! The bottom status bar, styled like VS Code's. The cursor position, language
//! and unsaved marker are live; the branch and indentation segments remain
//! mocked (they belong to later stories).

use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::domain::text_document::{DocumentContent, language_label};
use crate::presentation::editor_controller::{EditorTab, save_shortcut_label};
use crate::presentation::theme;

/// Radius of the unsaved ●, matching the explorer row's.
const DOT_RADIUS: f32 = 3.5;
/// Width of the slot the dot is painted in.
const DOT_SLOT: f32 = 12.0;

/// The live editor facts the status bar reports.
pub struct StatusInfo<'a> {
    pub project_name: &'a str,
    pub tab: Option<&'a EditorTab>,
    /// 1-based cursor position in the active buffer, when it has focus.
    pub cursor: Option<(usize, usize)>,
}

/// What the user did in the status bar this frame.
#[derive(Default)]
pub struct StatusBarEvents {
    /// The unsaved segment was clicked — save the active buffer.
    pub save: bool,
}

pub fn status_bar(ui: &mut egui::Ui, info: StatusInfo<'_>) -> StatusBarEvents {
    let mut events = StatusBarEvents::default();

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        segment(
            ui,
            &format!("{} main — {}", ph::GIT_BRANCH, info.project_name),
        );

        if let Some(tab) = info.tab
            && tab.unsaved()
        {
            events.save = unsaved_segment(ui, tab);
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

    events
}

/// The clickable "this buffer is unsaved" segment. Returns whether it was
/// pressed.
///
/// The dot is **painted**, not written as a `●` character: U+25CF is absent from
/// both the bundled text fonts and Phosphor (whose glyphs all live in the private
/// use area), so a literal one renders as a tofu box. The tab strip and the
/// explorer row paint theirs for the same reason — keep all three painted.
///
/// The segment names the file rather than just saying "unsaved changes": with
/// several dirty buffers open, the latter says nothing about which one Cmd+S is
/// about to write.
fn unsaved_segment(ui: &mut egui::Ui, tab: &EditorTab) -> bool {
    let inner = ui.horizontal(|ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(DOT_SLOT, DOT_RADIUS * 2.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), DOT_RADIUS, theme::UNSAVED_DOT);
        ui.label(
            RichText::new(format!("{} — {} to save", tab.name, save_shortcut_label()))
                .font(theme::small_font())
                .color(theme::UNSAVED_DOT),
        );
    });
    ui.add_space(10.0);

    // Own id rather than the layout's, so clicking the segment can't collide
    // with the horizontal's own interaction slot.
    let id = ui.make_persistent_id("status_bar_unsaved");
    let response = ui
        .interact(inner.response.rect, id, egui::Sense::click())
        .on_hover_text("Save this file");
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn segment(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::small_font())
            .color(theme::MUTED_ON_DARK),
    );
    ui.add_space(10.0);
}
