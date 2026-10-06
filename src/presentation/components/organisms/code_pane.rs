use eframe::egui;

use crate::application::ports::ClipboardPort;
use crate::domain::diagnostics::{Diagnostic, Severity, span_in};
use crate::domain::editing::{EditorCommand, find_matches};
use crate::domain::text_document::DocumentContent;
use crate::presentation::components::molecules::code_editor::{CodeEditorRequest, code_editor};
use crate::presentation::components::molecules::code_placeholder::{
    changed_on_disk_banner, code_placeholder, no_file_open,
};
use crate::presentation::components::molecules::find_bar::find_bar;
use crate::presentation::editor_controller::{EditorTab, TabId};
use crate::presentation::theme;

pub fn editor_id(tab: TabId) -> egui::Id {
    egui::Id::new(("code_editor", tab))
}

pub fn forget_editor_state(ctx: &egui::Context, tab: TabId) {
    ctx.data_mut(|data| data.remove::<egui::text_edit::TextEditState>(editor_id(tab)));
}

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
    injected: Option<EditorCommand>,
) -> CodePaneEvents {
    let mut events = CodePaneEvents::default();

    let Some(tab) = tab else {
        no_file_open(ui);
        return events;
    };

    if tab.external_change {
        ui.add_space(4.0);
        if changed_on_disk_banner(ui) {
            events.reload = true;
        }
        ui.add_space(4.0);
    }

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
            match_range = matches.get(find.current).cloned();
            if bar.changed || bar.next || bar.previous {
                select = match_range.clone();
            }
        }
        ui.add_space(4.0);
    }

    if let Some(span) = tab.pending_select.take() {
        select = Some(span);
    }

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
                    injected,
                },
            );
            events.edited = output.changed;
            events.cursor = output.cursor;
        }
        read_only => code_placeholder(ui, read_only),
    }

    events
}
