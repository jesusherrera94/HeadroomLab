//! The editor's central pane: whichever of the code editor, the read-only
//! placeholder or the empty state fits the active tab, with the find bar and the
//! changed-on-disk banner stacked above it.

use eframe::egui;

use crate::application::ports::ClipboardPort;
use crate::domain::editing::find_matches;
use crate::domain::text_document::DocumentContent;
use crate::presentation::components::molecules::code_editor::{CodeEditorRequest, code_editor};
use crate::presentation::components::molecules::code_placeholder::{
    changed_on_disk_banner, code_placeholder, no_file_open,
};
use crate::presentation::components::molecules::find_bar::find_bar;
use crate::presentation::editor_controller::{EditorTab, TabId};

/// The `TextEdit`'s id, and so the key under which egui keeps that buffer's
/// cursor, selection, scroll offset and undo history.
///
/// Keyed on [`TabId`] rather than on the path: a path is not stable — renaming a
/// file would change the id and silently drop the history — and it is not unique
/// over time either, so a closed-and-reopened file would inherit the undo stack
/// of its previous session. `TabId` is stable across a rename and never reused.
pub fn editor_id(tab: TabId) -> egui::Id {
    egui::Id::new(("code_editor", tab))
}

/// Drops a closed tab's editor state. egui's memory map has no GC, so without
/// this every tab ever opened keeps its buffer snapshot alive.
pub fn forget_editor_state(ctx: &egui::Context, tab: TabId) {
    ctx.data_mut(|data| data.remove::<egui::text_edit::TextEditState>(editor_id(tab)));
}

/// What happened in the code pane this frame.
#[derive(Default)]
pub struct CodePaneEvents {
    pub edited: bool,
    pub cursor: Option<(usize, usize)>,
    pub reload: bool,
    pub close_find: bool,
}

pub fn code_pane(
    ui: &mut egui::Ui,
    tab: Option<&mut EditorTab>,
    clipboard: &dyn ClipboardPort,
) -> CodePaneEvents {
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

    let id = editor_id(tab.id);
    let language = tab.language;

    // The buffer was just replaced from disk: drop the undo history with it, so
    // `⌘Z` cannot rewind to text this file no longer has.
    if std::mem::take(&mut tab.history_reset)
        && let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id)
    {
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }

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
                    clipboard,
                },
            );
            events.edited = output.changed;
            events.cursor = output.cursor;
        }
        read_only => code_placeholder(ui, read_only),
    }

    events
}
