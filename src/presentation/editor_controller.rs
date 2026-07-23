//! State and event handling for the Editor window (the IDE shell).
//!
//! HL9 is **layout only**: the explorer tree, tabs and terminal are static
//! mocks built from the opened project. The one live wire is the toolbar's
//! "Open emulator" button, whose intent the app turns into a simulator launch.
//! Real file/editor/terminal logic arrives in later tasks (Days 5–13).

use std::path::PathBuf;

use crate::domain::project::RecentProject;

/// Kind of a file/folder, used to pick its icon glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    Cpp,
    Header,
    Make,
    Markdown,
    Other,
}

/// One row in the (mocked) explorer tree. `depth` is the indentation level.
pub struct ExplorerNode {
    pub name: String,
    pub kind: FileKind,
    pub depth: usize,
}

/// One (mocked) open buffer shown in the tab strip.
pub struct EditorTab {
    pub name: String,
    pub kind: FileKind,
    pub unsaved: bool,
}

/// Per-window state for the Editor. All content besides the project identity is
/// a static mock in HL9.
pub struct EditorState {
    pub project_name: String,
    pub project_path: PathBuf,
    pub tree: Vec<ExplorerNode>,
    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,
    pub terminal_lines: Vec<String>,
    pub focus_requested: bool,
}

impl EditorState {
    /// Builds the mocked editor state for a freshly opened project. The tree
    /// mirrors the file set produced by the project generator (HL7) so the
    /// layout reads realistically; nothing is read from disk.
    pub fn new(project: &RecentProject) -> Self {
        let tree = vec![
            node(&project.name, FileKind::Folder, 0),
            node("Makefile", FileKind::Make, 1),
            node(".gitignore", FileKind::Other, 1),
            node("main.cpp", FileKind::Cpp, 1),
            node("effect_processor.h", FileKind::Header, 1),
            node("effect_processor.cpp", FileKind::Cpp, 1),
            node("dsp_primitives.h", FileKind::Header, 1),
            node("dsp_primitives.cpp", FileKind::Cpp, 1),
            node("hothouse_adapter.h", FileKind::Header, 1),
            node("hothouse_adapter.cpp", FileKind::Cpp, 1),
            node("hl_adapter.cpp", FileKind::Cpp, 1),
        ];

        let tabs = vec![
            EditorTab {
                name: "effect_processor.cpp".to_string(),
                kind: FileKind::Cpp,
                unsaved: false,
            },
            EditorTab {
                name: "effect_processor.h".to_string(),
                kind: FileKind::Header,
                unsaved: true,
            },
        ];

        let terminal_lines = vec![
            format!("{} $ make dylib", project.name),
            "  (build output will appear here)".to_string(),
        ];

        Self {
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            tree,
            tabs,
            active_tab: 0,
            terminal_lines,
            focus_requested: false,
        }
    }
}

fn node(name: &str, kind: FileKind, depth: usize) -> ExplorerNode {
    ExplorerNode {
        name: name.to_string(),
        kind,
        depth,
    }
}

/// What the user did in the Editor view this frame.
#[derive(Default)]
pub struct EditorViewEvents {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    pub tab_clicked: Option<usize>,
}

/// Follow-up actions the app must perform after handling the frame's events.
#[derive(Default)]
pub struct EditorRequests {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
}

/// Applies view events to the state and returns the actions the app must take.
pub fn handle_events(state: &mut EditorState, events: EditorViewEvents) -> EditorRequests {
    if let Some(index) = events.tab_clicked
        && index < state.tabs.len()
    {
        state.active_tab = index;
    }

    EditorRequests {
        open_emulator: events.open_emulator,
        build_run: events.build_run,
        compile: events.compile,
    }
}
