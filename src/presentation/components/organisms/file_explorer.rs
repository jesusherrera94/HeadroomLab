//! The explorer side panel: an "EXPLORER" header and the real project file
//! tree, read from disk on demand. Folders use a `CollapsingState` (persisted,
//! animated) and load their children the frame they're first opened. Returns
//! the user's explorer events for the controller to apply.

use eframe::egui::{self, RichText, collapsing_header::CollapsingState};

use crate::presentation::components::molecules::explorer_row::explorer_row;
use crate::presentation::editor_controller::{ExplorerEvents, FileTreeState, TreeNode};
use crate::presentation::theme;

pub fn file_explorer(ui: &mut egui::Ui, tree: &FileTreeState) -> ExplorerEvents {
    let mut events = ExplorerEvents::default();

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

    let selected = tree.selected.as_deref();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The project root defaults open; deeper folders default closed.
            render_node(ui, &tree.root, true, selected, &mut events);
        });

    events
}

fn render_node(
    ui: &mut egui::Ui,
    node: &TreeNode,
    default_open: bool,
    selected: Option<&std::path::Path>,
    events: &mut ExplorerEvents,
) {
    let is_selected = selected == Some(node.path.as_path());

    if !node.is_dir {
        let response = explorer_row(ui, node, None, is_selected);
        if response.clicked() {
            events.select = Some(node.path.clone());
        }
        return;
    }

    let id = ui.make_persistent_id(&node.path);
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);

    let response = explorer_row(ui, node, Some(state.is_open()), is_selected);
    if response.clicked() {
        state.toggle(ui);
    }

    // Reading happens after the (possible) toggle so a freshly opened folder is
    // scheduled to load this frame; its children fill in on the next paint.
    if state.is_open() && !node.loaded {
        events.expand.push(node.path.clone());
    }

    state.show_body_indented(&response, ui, |ui| {
        for child in &node.children {
            render_node(ui, child, false, selected, events);
        }
    });
}
