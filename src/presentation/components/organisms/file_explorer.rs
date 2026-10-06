use std::collections::HashSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, collapsing_header::CollapsingState};
use egui_phosphor::regular as ph;

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::components::molecules::explorer_row::explorer_row;
use crate::presentation::editor_controller::{
    self, EntryKind, ExplorerEvents, ExplorerUiState, FileTreeState, TreeNode, entry_kind_icon,
    reveal_label,
};
use crate::presentation::theme;

const CARET_WIDTH: f32 = 14.0;
pub fn file_explorer(
    ui: &mut egui::Ui,
    tree: &FileTreeState,
    ui_state: &mut ExplorerUiState,
    unsaved: &HashSet<PathBuf>,
) -> ExplorerEvents {
    let mut events = ExplorerEvents::default();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("EXPLORER")
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            if icon_button(ui, ph::FOLDER_PLUS, "New Folder").clicked() {
                events.begin_create =
                    Some((editor_controller::create_target(tree), EntryKind::Directory));
            }
            if icon_button(ui, ph::FILE_PLUS, "New File").clicked() {
                events.begin_create =
                    Some((editor_controller::create_target(tree), EntryKind::File));
            }
        });
    });
    ui.add_space(4.0);

    let selected = tree.selected.clone();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_node(
                ui,
                &tree.root,
                true,
                selected.as_deref(),
                unsaved,
                ui_state,
                &mut events,
            );

            let remaining = ui.available_size();
            if remaining.y > 4.0 {
                let bg = ui.allocate_response(remaining, egui::Sense::click());
                bg.context_menu(|ui| {
                    ui.set_min_width(170.0);
                    if ui.button("New File").clicked() {
                        events.begin_create = Some((tree.root.path.clone(), EntryKind::File));
                        ui.close();
                    }
                    if ui.button("New Folder").clicked() {
                        events.begin_create = Some((tree.root.path.clone(), EntryKind::Directory));
                        ui.close();
                    }
                });
            }
        });

    events
}

fn render_node(
    ui: &mut egui::Ui,
    node: &TreeNode,
    default_open: bool,
    selected: Option<&Path>,
    unsaved: &HashSet<PathBuf>,
    ui_state: &mut ExplorerUiState,
    events: &mut ExplorerEvents,
) {
    let is_selected = selected == Some(node.path.as_path());
    let renaming = ui_state
        .pending_rename
        .as_ref()
        .is_some_and(|r| r.path == node.path);

    if !node.is_dir {
        if renaming {
            inline_rename_row(ui, node, None, ui_state, events);
        } else {
            let row = explorer_row(ui, node, None, is_selected, unsaved.contains(&node.path));
            if row.double_clicked() {
                events.begin_rename = Some(node.path.clone());
            } else if row.clicked() {
                events.open = Some(node.path.clone());
            }
            row.context_menu(|ui| entry_menu(ui, node, events));
        }
        return;
    }

    let id = ui.make_persistent_id(&node.path);
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);

    let creating_here = ui_state
        .pending_create
        .as_ref()
        .is_some_and(|c| c.parent == node.path);
    if creating_here {
        state.set_open(true);
    }

    let header = if renaming {
        inline_rename_row(ui, node, Some(state.is_open()), ui_state, events)
    } else {
        let row = explorer_row(ui, node, Some(state.is_open()), is_selected, false);
        if row.double_clicked() {
            events.begin_rename = Some(node.path.clone());
        } else if row.clicked() {
            events.select = Some(node.path.clone());
            state.toggle(ui);
        }
        row.context_menu(|ui| entry_menu(ui, node, events));
        row
    };

    if state.is_open() && !node.loaded {
        events.expand.push(node.path.clone());
    }

    state.show_body_indented(&header, ui, |ui| {
        if creating_here {
            inline_create_row(ui, ui_state, events);
        }
        for child in &node.children {
            render_node(ui, child, false, selected, unsaved, ui_state, events);
        }
    });
}

fn inline_rename_row(
    ui: &mut egui::Ui,
    node: &TreeNode,
    open: Option<bool>,
    ui_state: &mut ExplorerUiState,
    events: &mut ExplorerEvents,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(CARET_WIDTH);
        file_icon(ui, editor_controller::row_icon(node, open));
        let Some(rename) = ui_state.pending_rename.as_mut() else {
            return;
        };
        let editor = ui.add(
            egui::TextEdit::singleline(&mut rename.buffer)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Body),
        );
        if rename.focus {
            editor.request_focus();
            rename.focus = false;
        }
        let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if editor.lost_focus() {
            if escaped {
                events.cancel_rename = true;
            } else {
                events.commit_rename = true;
            }
        }
    })
    .response
}

fn inline_create_row(
    ui: &mut egui::Ui,
    ui_state: &mut ExplorerUiState,
    events: &mut ExplorerEvents,
) {
    ui.horizontal(|ui| {
        let Some(create) = ui_state.pending_create.as_mut() else {
            return;
        };
        ui.add_space(CARET_WIDTH);
        file_icon(ui, entry_kind_icon(create.kind));
        let hint = match create.kind {
            EntryKind::File => "new file name",
            EntryKind::Directory => "new folder name",
        };
        let editor = ui.add(
            egui::TextEdit::singleline(&mut create.buffer)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
                .font(egui::TextStyle::Body),
        );
        if create.focus {
            editor.request_focus();
            create.focus = false;
        }
        let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if editor.lost_focus() {
            if escaped || create.buffer.trim().is_empty() {
                events.cancel_create = true;
            } else {
                events.commit_create = true;
            }
        }
    });
}

fn entry_menu(ui: &mut egui::Ui, node: &TreeNode, events: &mut ExplorerEvents) {
    ui.set_min_width(190.0);
    let dir = if node.is_dir {
        node.path.clone()
    } else {
        node.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| node.path.clone())
    };

    if menu_item(ui, ph::FILE_PLUS, "New File").clicked() {
        events.begin_create = Some((dir.clone(), EntryKind::File));
        ui.close();
    }
    if menu_item(ui, ph::FOLDER_PLUS, "New Folder").clicked() {
        events.begin_create = Some((dir, EntryKind::Directory));
        ui.close();
    }
    ui.separator();
    if menu_item(ui, ph::PENCIL_SIMPLE, "Rename").clicked() {
        events.begin_rename = Some(node.path.clone());
        ui.close();
    }
    if menu_item(ui, ph::TRASH, "Delete").clicked() {
        events.request_delete = Some(node.path.clone());
        ui.close();
    }
    ui.separator();
    if menu_item(ui, ph::ARROW_SQUARE_OUT, reveal_label()).clicked() {
        events.reveal = Some(node.path.clone());
        ui.close();
    }
}

fn menu_item(ui: &mut egui::Ui, glyph: &str, label: &str) -> egui::Response {
    ui.add(egui::Button::new(format!("{glyph}   {label}")).frame(false))
}

fn icon_button(ui: &mut egui::Ui, glyph: &str, tooltip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(glyph)
                .font(egui::FontId::proportional(theme::FONT_SUBTITLE))
                .color(theme::MUTED_ON_DARK),
        )
        .frame(false),
    )
    .on_hover_text(tooltip)
}
