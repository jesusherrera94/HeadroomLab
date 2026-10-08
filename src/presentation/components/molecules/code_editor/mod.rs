mod commands;
mod context_menu;
mod keystrokes;
mod squiggles;

use std::ops::Range;

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};

use crate::application::ports::ClipboardPort;
use crate::domain::editing::EditorCommand;
use crate::domain::text_document::{Language, line_col_at};
use crate::presentation::components::atoms::line_numbers::{gutter_width, line_numbers};
use crate::presentation::syntax;

use commands::run_command;
use context_menu::editor_menu;
use keystrokes::rewrite_events;
use squiggles::paint_squiggles;

#[derive(Default)]
pub struct CodeEditorOutput {
    pub changed: bool,
    pub cursor: Option<(usize, usize)>,
}

pub struct CodeEditorRequest<'a> {
    pub text: &'a mut String,
    pub language: Language,
    pub highlighted: bool,
    pub id: egui::Id,
    pub select: Option<Range<usize>>,
    pub match_range: Option<Range<usize>>,
    pub clipboard: &'a dyn ClipboardPort,
    pub squiggles: &'a [(Range<usize>, egui::Color32)],
    pub injected: Option<EditorCommand>,
}

pub fn code_editor(ui: &mut egui::Ui, request: CodeEditorRequest<'_>) -> CodeEditorOutput {
    let CodeEditorRequest {
        text,
        language,
        highlighted,
        id,
        select,
        match_range,
        clipboard,
        squiggles,
        injected,
    } = request;

    let pending = rewrite_events(ui, id, text, language, clipboard);

    let gutter = gutter_width(ui, text.lines().count().max(1));
    let mut result = CodeEditorOutput::default();

    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (gutter_rect, _) =
                    ui.allocate_exact_size(egui::vec2(gutter, 0.0), egui::Sense::hover());

                let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _wrap: f32| {
                    syntax::layouter(
                        ui,
                        buffer.as_str(),
                        language,
                        highlighted,
                        match_range.as_ref(),
                    )
                };

                let mut output = egui::TextEdit::multiline(text)
                    .id(id)
                    .font(syntax::code_font())
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter)
                    .show(ui);

                line_numbers(ui, gutter_rect, &output.galley, output.galley_pos.y);

                result.changed = output.response.response.changed();

                let paste_id = id.with("can_paste");
                if output.response.response.secondary_clicked() {
                    let available = clipboard.read().is_some();
                    ui.data_mut(|data| data.insert_temp(paste_id, available));
                }

                let mut chosen = None;
                output.response.response.context_menu(|ui| {
                    let can_paste = ui.data(|data| data.get_temp(paste_id).unwrap_or(false));
                    chosen = editor_menu(ui, text, id, language, can_paste);
                });
                let chosen = chosen.or(injected);

                let mut menu_outcome = None;
                if let Some(command) = chosen {
                    ui.memory_mut(|memory| memory.request_focus(id));
                    menu_outcome = run_command(ui, id, text, language, clipboard, command);
                    result.changed |= menu_outcome.as_ref().is_some_and(|o| o.changed);
                }

                paint_squiggles(ui, &output.galley, output.galley_pos, squiggles);

                if pending.step_back_one
                    && let Some(range) = output.state.cursor.char_range()
                {
                    let back = CCursor::new(range.primary.index.0.saturating_sub(1));
                    output
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(back)));
                    output.state.clone().store(ui.ctx(), id);
                }

                result.changed |= pending.outcome.as_ref().is_some_and(|o| o.changed);

                let selection = menu_outcome
                    .or(pending.outcome)
                    .map(|outcome| (outcome.selection, outcome.scroll))
                    .or_else(|| select.map(|range| (range, true)));

                if let Some((range, scroll)) = selection {
                    output.state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(range.start),
                        CCursor::new(range.end),
                    )));
                    output.state.clone().store(ui.ctx(), id);
                    if scroll
                        && let Some(row) =
                            output.galley.rows.get(line_col_at(text, range.start).0 - 1)
                    {
                        ui.scroll_to_rect(
                            row.rect().translate(output.galley_pos.to_vec2()),
                            Some(egui::Align::Center),
                        );
                    }
                }

                result.cursor = output
                    .state
                    .cursor
                    .char_range()
                    .map(|range| line_col_at(text, range.primary.index.0));
            });
        });

    result
}
