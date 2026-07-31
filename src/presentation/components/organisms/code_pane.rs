//! The editor's central pane: whichever of the code editor, the read-only
//! placeholder or the empty state fits the active tab, with the find bar and the
//! changed-on-disk banner stacked above it.

use eframe::egui;

use crate::application::ports::ClipboardPort;
use crate::domain::diagnostics::{Diagnostic, Severity, span_in};
use crate::domain::editing::find_matches;
use crate::domain::text_document::DocumentContent;
use crate::presentation::components::molecules::code_editor::{CodeEditorRequest, code_editor};
use crate::presentation::components::molecules::code_placeholder::{
    changed_on_disk_banner, code_placeholder, no_file_open,
};
use crate::presentation::components::molecules::find_bar::find_bar;
use crate::presentation::editor_controller::{EditorTab, TabId};
use crate::presentation::theme;

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
    diagnostics: &[Diagnostic],
    stale: bool,
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
    let mut match_range = None;
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
            // Tinted every frame the bar is open, so the hit stays visible while
            // the user keeps typing — the selection alone would not be, since
            // egui paints one only for the focused widget and the focus is in
            // the query field.
            match_range = matches.get(find.current).cloned();
            // Scrolled into view only when the hit actually changed.
            if bar.changed || bar.next || bar.previous {
                select = match_range.clone();
            }
        }
        ui.add_space(4.0);
    }

    // A jump from the problems strip wins over a find hit: they cannot both
    // happen in one frame, and taking it here consumes it so the caret moves
    // once rather than on every later paint.
    if let Some(span) = tab.pending_select.take() {
        select = Some(span);
    }

    // Squiggles for this buffer. Dropped entirely once the file has been edited:
    // the compiler's line numbers describe text that no longer exists, so a mark
    // would sit under whatever moved into that line.
    let squiggles: Vec<(std::ops::Range<usize>, egui::Color32)> = match (stale, tab.content.text())
    {
        (false, Some(text)) => diagnostics
            .iter()
            .filter_map(|d| {
                let span = span_in(text, d.line?, d.column)?;
                let color = match d.severity {
                    Severity::Error => theme::DIAGNOSTIC_ERROR,
                    Severity::Warning => theme::DIAGNOSTIC_WARNING,
                };
                Some((span, color))
            })
            .collect(),
        _ => Vec::new(),
    };

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
                    match_range,
                    clipboard,
                    squiggles: &squiggles,
                },
            );
            events.edited = output.changed;
            events.cursor = output.cursor;
        }
        read_only => code_placeholder(ui, read_only),
    }

    events
}
