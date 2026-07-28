//! The editor's central pane: whichever of the code editor, the read-only
//! placeholder or the empty state fits the active tab, with the find bar and the
//! changed-on-disk banner stacked above it.

use eframe::egui;

use crate::domain::text_document::DocumentContent;
use crate::presentation::components::molecules::code_editor::{CodeEditorRequest, code_editor};
use crate::presentation::components::molecules::code_placeholder::{
    changed_on_disk_banner, code_placeholder, no_file_open,
};
use crate::presentation::components::molecules::find_bar::{find_bar, find_matches};
use crate::presentation::editor_controller::EditorTab;

/// What happened in the code pane this frame.
#[derive(Default)]
pub struct CodePaneEvents {
    pub edited: bool,
    pub cursor: Option<(usize, usize)>,
    pub reload: bool,
    pub close_find: bool,
}

pub fn code_pane(ui: &mut egui::Ui, tab: Option<&mut EditorTab>) -> CodePaneEvents {
    let mut events = CodePaneEvents::default();

    let Some(tab) = tab else {
        no_file_open(ui);
        return events;
    };

    // Banner first: it must be visible without scrolling the buffer.
    if tab.external_change {
        ui.add_space(4.0);
        if changed_on_disk_banner(ui) {
            events.reload = true;
        }
        ui.add_space(4.0);
    }

    // Resolve find hits before borrowing the text mutably for the editor.
    let mut select = None;
    if let (Some(find), Some(text)) = (tab.find.as_mut(), tab.content.text()) {
        let matches = find_matches(text, &find.query);
        let bar = find_bar(ui, find, matches.len());
        if bar.close {
            events.close_find = true;
        }
        if !matches.is_empty() {
            if bar.changed {
                find.current = 0;
            } else if bar.next {
                find.current = (find.current + 1) % matches.len();
            } else if bar.previous {
                find.current = (find.current + matches.len() - 1) % matches.len();
            }
            // Re-select on any navigation, and whenever the query changes.
            if bar.changed || bar.next || bar.previous {
                select = matches.get(find.current).cloned();
            }
        }
        ui.add_space(4.0);
    }

    let id = egui::Id::new(("code_editor", &tab.path));
    let language = tab.language;

    match &mut tab.content {
        DocumentContent::Text {
            text, highlight, ..
        } => {
            let output = code_editor(
                ui,
                CodeEditorRequest {
                    text,
                    language,
                    highlighted: *highlight,
                    id,
                    select,
                },
            );
            events.edited = output.changed;
            events.cursor = output.cursor;
        }
        read_only => code_placeholder(ui, read_only),
    }

    events
}
