//! The menu bar's vocabulary and its enablement rules.
//!
//! Pure data: nothing here knows about egui, muda or the OS. The module answers
//! one question — given what the app is doing right now, what does the menu look
//! like — and *both* view adapters render the same answer, so the native macOS
//! bar and the in-window egui bar cannot drift apart in what they offer or in
//! what they grey out.
//!
//! The commands are deliberately a thin vocabulary rather than behaviour. Every
//! edit command is a [`EditorCommand`], the same one a key chord or the code
//! area's context menu produces, so the menu bar is a *third* entry point into
//! an existing path rather than a second implementation of it.

use std::fmt;

use crate::domain::editing::EditorCommand;

/// Every window the app can put on screen. Doubles as the Window menu's entries
/// and as "which window is focused" in [`MenuContext`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowId {
    Splash,
    Initial,
    Editor,
    Simulator,
    Graph,
    Doom,
}

impl WindowId {
    /// How this window is named in the Window menu.
    pub fn menu_label(self) -> &'static str {
        match self {
            WindowId::Splash | WindowId::Initial => "HeadroomLab",
            WindowId::Editor => "Editor",
            WindowId::Simulator => "Hardware Simulator",
            WindowId::Graph => "Signal Graph",
            WindowId::Doom => "DOOM.666",
        }
    }
}

/// Which bar is being built.
///
/// `Global` is the one macOS menu bar shared by the whole app, so its titles are
/// fixed and items grey out per focus (D4). `Window` is a per-window bar, which
/// exists only on Windows and Linux and carries just the menus that window can
/// act on (D14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSurface {
    Global,
    Window(WindowId),
}

/// Items the OS implements itself. Carried in the model so both adapters agree
/// on where they sit, but only the macOS adapter ever renders one — muda lists
/// every one of these as unsupported off macOS, and a window's own title bar
/// already provides Minimize and Zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredefinedItem {
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    BringAllToFront,
}

/// The Simulator's transport actions, mirroring `TransportEvents`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCommand {
    LoadAudio,
    PlayPause,
    Bypass,
    ViewGraph,
}

/// Everything the menu bar can ask the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    About,
    Help,
    Quit,

    NewProject,
    OpenProject,
    /// Open the recent project at this index in [`MenuContext::recents`].
    OpenRecent(usize),
    ClearRecents,

    NewFile,
    NewFolder,
    Save,
    SaveAll,
    /// Close the active tab (⌘W in the Editor).
    CloseTab,
    /// Close the focused window. In the Editor that is a quit, and goes through
    /// the same unsaved-work guard.
    CloseWindow,

    /// An edit command, forwarded verbatim into the code editor's existing path.
    Edit(EditorCommand),
    Find,

    OpenEmulator,
    BuildRun,
    Compile,

    Transport(TransportCommand),

    /// Focus that window, opening it first if it is closed (D9).
    FocusWindow(WindowId),

    /// Handed to the OS. Never dispatched by the controller.
    Predefined(PredefinedItem),
}

/// A keyboard chord, stored once so the menus and the code editor's context menu
/// render the same text. `command` is ⌘ on macOS and Ctrl everywhere else, which
/// is the same mapping `egui::Modifiers::COMMAND` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
    /// The key's non-macOS spelling — `"S"`, `"/"`, `"Down"`.
    pub key: &'static str,
}

impl Chord {
    pub const fn cmd(key: &'static str) -> Self {
        Self {
            command: true,
            shift: false,
            alt: false,
            key,
        }
    }

    pub const fn cmd_shift(key: &'static str) -> Self {
        Self {
            command: true,
            shift: true,
            alt: false,
            key,
        }
    }

    pub const fn cmd_alt(key: &'static str) -> Self {
        Self {
            command: true,
            shift: false,
            alt: true,
            key,
        }
    }

    pub const fn cmd_alt_shift(key: &'static str) -> Self {
        Self {
            command: true,
            shift: true,
            alt: true,
            key,
        }
    }

    pub const fn shift_alt(key: &'static str) -> Self {
        Self {
            command: false,
            shift: true,
            alt: true,
            key,
        }
    }

    pub const fn plain(key: &'static str) -> Self {
        Self {
            command: false,
            shift: false,
            alt: false,
            key,
        }
    }
}

/// macOS writes modifiers as glyphs in the order ⇧⌥⌘ and takes no separator;
/// Windows and Linux spell them out and join with `+`.
impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if cfg!(target_os = "macos") {
            if self.shift {
                f.write_str("⇧")?;
            }
            if self.alt {
                f.write_str("⌥")?;
            }
            if self.command {
                f.write_str("⌘")?;
            }
            f.write_str(mac_key(self.key))
        } else {
            if self.command {
                f.write_str("Ctrl+")?;
            }
            if self.shift {
                f.write_str("Shift+")?;
            }
            if self.alt {
                f.write_str("Alt+")?;
            }
            f.write_str(self.key)
        }
    }
}

/// The few keys macOS draws as an arrow rather than a word.
fn mac_key(key: &str) -> &str {
    match key {
        "Down" => "↓",
        "Up" => "↑",
        other => other,
    }
}

// -- The chord table ------------------------------------------------------
//
// One table, used by the menus *and* by the code editor's context menu, so a
// shortcut can never be advertised two different ways.

pub const SAVE: Chord = Chord::cmd("S");
pub const SAVE_ALL: Chord = Chord::cmd_shift("S");
pub const FIND: Chord = Chord::cmd("F");
pub const NEW_PROJECT: Chord = Chord::cmd_shift("N");
pub const OPEN_PROJECT: Chord = Chord::cmd("O");
pub const NEW_FILE: Chord = Chord::cmd("N");
pub const NEW_FOLDER: Chord = Chord::cmd_alt_shift("N");
pub const CLOSE_TAB: Chord = Chord::cmd("W");
pub const CLOSE_WINDOW: Chord = Chord::cmd_shift("W");
pub const QUIT: Chord = Chord::cmd("Q");
pub const HIDE: Chord = Chord::cmd("H");
pub const HIDE_OTHERS: Chord = Chord::cmd_alt("H");
pub const MINIMIZE: Chord = Chord::cmd("M");
pub const BUILD_RUN: Chord = Chord::cmd("R");
pub const COMPILE: Chord = Chord::cmd("B");
pub const PLAY_PAUSE: Chord = Chord::plain("Space");

pub const UNDO: Chord = Chord::cmd("Z");
pub const REDO: Chord = Chord::cmd_shift("Z");
pub const CUT: Chord = Chord::cmd("X");
pub const COPY: Chord = Chord::cmd("C");
pub const PASTE: Chord = Chord::cmd("V");
pub const SELECT_ALL: Chord = Chord::cmd("A");
pub const SELECT_LINE: Chord = Chord::cmd("L");
pub const SELECT_NEXT: Chord = Chord::cmd("D");
pub const DUPLICATE_LINE: Chord = Chord::shift_alt("Down");
pub const DELETE_LINE: Chord = Chord::cmd_shift("K");
pub const TOGGLE_COMMENT: Chord = Chord::cmd("/");

/// The chord for an editor command, so the code editor's context menu and the
/// Edit menu label the same action identically.
pub fn chord_for(command: EditorCommand) -> Chord {
    match command {
        EditorCommand::Undo => UNDO,
        EditorCommand::Redo => REDO,
        EditorCommand::Cut => CUT,
        EditorCommand::Copy => COPY,
        EditorCommand::Paste => PASTE,
        EditorCommand::SelectAll => SELECT_ALL,
        EditorCommand::SelectLine => SELECT_LINE,
        EditorCommand::SelectNextOccurrence => SELECT_NEXT,
        EditorCommand::ToggleComment => TOGGLE_COMMENT,
        EditorCommand::DuplicateLine => DUPLICATE_LINE,
        EditorCommand::DeleteLine => DELETE_LINE,
    }
}

// -- The model ------------------------------------------------------------

/// One row of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<Chord>,
    pub command: MenuCommand,
    pub enabled: bool,
    /// `Some` for checkable items (Bypass, and the focused window's tick).
    pub checked: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuItem),
    Separator,
    Submenu(Menu),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub entries: Vec<MenuEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuModel {
    pub menus: Vec<Menu>,
}

/// Everything the enablement rules need, collected once per frame.
///
/// Deliberately flat and owned: it is compared against the previous frame's
/// context to decide whether the native menu needs rebuilding at all (S6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuContext {
    pub focused: WindowId,
    pub has_project: bool,
    /// Labels for the Open Recent submenu, in `RecentProjectsService` order.
    pub recents: Vec<String>,

    /// Whether the code area itself holds keyboard focus.
    ///
    /// The Edit menu stands down unless it does. On macOS an enabled item's
    /// accelerator is consumed by the menu *before* the window sees the key, so
    /// an Edit menu that stayed live regardless of focus would swallow `⌘C`,
    /// `⌘V` and `⌘A` from every other field in the window — the terminal (whose
    /// copy binding is `⌘C`, because a bare `Ctrl+C` has to stay SIGINT), the
    /// find bar's query box, and the explorer's inline rename — and apply them
    /// to the buffer behind them instead. This is the same rule macOS applies
    /// through the responder chain, which an egui-painted window cannot use.
    pub code_area_focused: bool,

    pub has_open_tab: bool,
    /// False for binary/oversized buffers, which are shown but never edited.
    pub active_tab_editable: bool,
    pub active_tab_dirty: bool,
    pub any_tab_dirty: bool,
    pub has_selection: bool,
    pub can_paste: bool,
    /// Whether the active buffer's language has a line-comment token.
    pub can_comment: bool,

    pub build_running: bool,

    pub simulator_open: bool,
    pub graph_open: bool,
    pub doom_open: bool,
    pub has_audio: bool,
    pub is_playing: bool,
    pub is_bypassed: bool,
}

impl Default for MenuContext {
    fn default() -> Self {
        Self {
            focused: WindowId::Splash,
            has_project: false,
            recents: Vec::new(),
            code_area_focused: false,
            has_open_tab: false,
            active_tab_editable: false,
            active_tab_dirty: false,
            any_tab_dirty: false,
            has_selection: false,
            can_paste: false,
            can_comment: false,
            build_running: false,
            simulator_open: false,
            graph_open: false,
            doom_open: false,
            has_audio: false,
            is_playing: false,
            is_bypassed: false,
        }
    }
}

impl MenuModel {
    /// Builds the menu for `surface`.
    ///
    /// On a per-window surface the focus is taken to *be* that window: the bar is
    /// drawn inside it, and clicking a bar focuses its window as a side effect,
    /// so a Simulator bar that greyed itself out because the Editor happened to
    /// hold focus would be unusable.
    pub fn build(ctx: &MenuContext, surface: MenuSurface) -> Self {
        let ctx = match surface {
            MenuSurface::Global => ctx.clone(),
            MenuSurface::Window(window) => MenuContext {
                focused: window,
                ..ctx.clone()
            },
        };

        let menus = match surface {
            MenuSurface::Global => vec![
                app_menu(),
                file_menu(&ctx, surface),
                edit_menu(&ctx),
                build_menu(&ctx),
                transport_menu(&ctx),
                window_menu(&ctx, surface),
                help_menu(),
            ],
            // D14: only the menus this window can act on. The App menu has no
            // separate home off macOS, so About and Quit fold into File.
            MenuSurface::Window(WindowId::Editor) => vec![
                file_menu(&ctx, surface),
                edit_menu(&ctx),
                build_menu(&ctx),
                window_menu(&ctx, surface),
                help_menu(),
            ],
            MenuSurface::Window(WindowId::Simulator) => vec![
                file_menu(&ctx, surface),
                transport_menu(&ctx),
                window_menu(&ctx, surface),
                help_menu(),
            ],
            MenuSurface::Window(WindowId::Graph) => vec![
                file_menu(&ctx, surface),
                window_menu(&ctx, surface),
                help_menu(),
            ],
            // Splash, Initial and DOOM draw no strip (S4).
            MenuSurface::Window(_) => Vec::new(),
        };

        Self { menus }
    }

    /// The top-level titles, in order. The D4 invariant is asserted against this.
    pub fn titles(&self) -> Vec<&str> {
        self.menus.iter().map(|m| m.title.as_str()).collect()
    }

    /// The first item carrying `command`, wherever it sits.
    pub fn find(&self, command: MenuCommand) -> Option<&MenuItem> {
        self.menus.iter().find_map(|menu| find_in(menu, command))
    }

    /// Whether `command` is enabled. Missing items count as disabled, which is
    /// what a caller asking "can I do this?" means either way.
    pub fn is_enabled(&self, command: MenuCommand) -> bool {
        self.find(command).is_some_and(|item| item.enabled)
    }
}

fn find_in(menu: &Menu, command: MenuCommand) -> Option<&MenuItem> {
    menu.entries.iter().find_map(|entry| match entry {
        MenuEntry::Item(item) if item.command == command => Some(item),
        MenuEntry::Submenu(sub) => find_in(sub, command),
        _ => None,
    })
}

// -- Builders -------------------------------------------------------------

fn item(label: impl Into<String>, command: MenuCommand, enabled: bool) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.into(),
        shortcut: None,
        command,
        enabled,
        checked: None,
    })
}

fn keyed(label: impl Into<String>, chord: Chord, command: MenuCommand, enabled: bool) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.into(),
        shortcut: Some(chord),
        command,
        enabled,
        checked: None,
    })
}

fn checkable(
    label: impl Into<String>,
    command: MenuCommand,
    enabled: bool,
    checked: bool,
) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.into(),
        shortcut: None,
        command,
        enabled,
        checked: Some(checked),
    })
}

fn predefined(label: &str, chord: Option<Chord>, which: PredefinedItem) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.to_string(),
        shortcut: chord,
        command: MenuCommand::Predefined(which),
        enabled: true,
        checked: None,
    })
}

/// The macOS application menu. Never built for a per-window surface.
fn app_menu() -> Menu {
    Menu {
        title: "HeadroomLab".to_string(),
        entries: vec![
            item("About HeadroomLab", MenuCommand::About, true),
            MenuEntry::Separator,
            predefined("Services", None, PredefinedItem::Services),
            predefined("Hide HeadroomLab", Some(HIDE), PredefinedItem::Hide),
            predefined("Hide Others", Some(HIDE_OTHERS), PredefinedItem::HideOthers),
            predefined("Show All", None, PredefinedItem::ShowAll),
            MenuEntry::Separator,
            // Custom, never `PredefinedMenuItem::quit()` — that calls
            // `NSApp terminate:` and would walk straight past the unsaved-work
            // confirmation (D8).
            keyed("Quit HeadroomLab", QUIT, MenuCommand::Quit, true),
        ],
    }
}

/// D12: during the splash, and while DOOM holds focus, the whole File menu greys
/// out bar Close Window — DOOM owns the keyboard, and nothing there should be
/// able to touch the project.
fn file_active(ctx: &MenuContext) -> bool {
    matches!(ctx.focused, WindowId::Initial | WindowId::Editor)
}

fn file_menu(ctx: &MenuContext, surface: MenuSurface) -> Menu {
    let active = file_active(ctx);
    let editing = active && ctx.has_project;

    let mut entries = vec![
        keyed("New Project…", NEW_PROJECT, MenuCommand::NewProject, active),
        keyed(
            "Open Project…",
            OPEN_PROJECT,
            MenuCommand::OpenProject,
            active,
        ),
        MenuEntry::Submenu(recents_menu(ctx, active)),
        MenuEntry::Separator,
        keyed("New File", NEW_FILE, MenuCommand::NewFile, editing),
        keyed("New Folder", NEW_FOLDER, MenuCommand::NewFolder, editing),
        MenuEntry::Separator,
        keyed(
            "Save",
            SAVE,
            MenuCommand::Save,
            active && ctx.active_tab_dirty,
        ),
        keyed(
            "Save All",
            SAVE_ALL,
            MenuCommand::SaveAll,
            active && ctx.any_tab_dirty,
        ),
        MenuEntry::Separator,
        keyed(
            "Close Tab",
            CLOSE_TAB,
            MenuCommand::CloseTab,
            ctx.focused == WindowId::Editor && ctx.has_open_tab,
        ),
        // Always available: whatever else is greyed, the user can always get out
        // of the window they are looking at.
        //
        // The chord moves with the focus (D5). Only the Editor has tabs, so only
        // there does ⌘W mean "close the tab" and closing the window take ⇧⌘W; in
        // the Simulator, Graph and DOOM the reflexive ⌘W closes the window
        // itself, as it does in every other Mac app. The two items therefore
        // never claim ⌘W at the same time.
        keyed(
            "Close Window",
            if ctx.focused == WindowId::Editor {
                CLOSE_WINDOW
            } else {
                CLOSE_TAB
            },
            MenuCommand::CloseWindow,
            true,
        ),
    ];

    // Off macOS there is no separate application menu, so its two items that
    // actually do something live at the foot of File.
    if matches!(surface, MenuSurface::Window(_)) {
        entries.push(MenuEntry::Separator);
        entries.push(item("About HeadroomLab", MenuCommand::About, true));
        entries.push(keyed("Quit HeadroomLab", QUIT, MenuCommand::Quit, true));
    }

    Menu {
        title: "File".to_string(),
        entries,
    }
}

fn recents_menu(ctx: &MenuContext, active: bool) -> Menu {
    let mut entries: Vec<MenuEntry> = ctx
        .recents
        .iter()
        .enumerate()
        .map(|(index, label)| item(label.clone(), MenuCommand::OpenRecent(index), active))
        .collect();

    if !entries.is_empty() {
        entries.push(MenuEntry::Separator);
        entries.push(item("Clear Menu", MenuCommand::ClearRecents, active));
    }

    Menu {
        title: "Open Recent".to_string(),
        entries,
    }
}

fn edit_menu(ctx: &MenuContext) -> Menu {
    // The Edit menu acts on the code area, so it needs the Editor focused with
    // something open in it. Mutating commands additionally need a buffer that is
    // editable at all — binary and oversized documents are shown, never written.
    let base = ctx.focused == WindowId::Editor && ctx.has_open_tab && ctx.code_area_focused;
    let writable = base && ctx.active_tab_editable;

    let edit = |label: &str, command: EditorCommand, enabled: bool| {
        keyed(
            label,
            chord_for(command),
            MenuCommand::Edit(command),
            enabled,
        )
    };

    Menu {
        title: "Edit".to_string(),
        entries: vec![
            edit("Undo", EditorCommand::Undo, writable),
            edit("Redo", EditorCommand::Redo, writable),
            MenuEntry::Separator,
            edit("Cut", EditorCommand::Cut, writable && ctx.has_selection),
            edit("Copy", EditorCommand::Copy, base && ctx.has_selection),
            edit("Paste", EditorCommand::Paste, writable && ctx.can_paste),
            edit("Select All", EditorCommand::SelectAll, base),
            MenuEntry::Separator,
            edit("Select Line", EditorCommand::SelectLine, base),
            edit(
                "Select Next Occurrence",
                EditorCommand::SelectNextOccurrence,
                base,
            ),
            edit("Duplicate Line", EditorCommand::DuplicateLine, writable),
            edit("Delete Line", EditorCommand::DeleteLine, writable),
            edit(
                "Toggle Comment",
                EditorCommand::ToggleComment,
                writable && ctx.can_comment,
            ),
            MenuEntry::Separator,
            keyed("Find…", FIND, MenuCommand::Find, writable),
        ],
    }
}

fn build_menu(ctx: &MenuContext) -> Menu {
    let active = ctx.focused == WindowId::Editor && ctx.has_project;
    let can_build = active && !ctx.build_running;

    Menu {
        title: "Build".to_string(),
        entries: vec![
            item("Open Emulator", MenuCommand::OpenEmulator, active),
            keyed(
                "Build & Run Emulator",
                BUILD_RUN,
                MenuCommand::BuildRun,
                can_build,
            ),
            keyed("Compile", COMPILE, MenuCommand::Compile, can_build),
        ],
    }
}

fn transport_menu(ctx: &MenuContext) -> Menu {
    let active = ctx.focused == WindowId::Simulator;
    let loaded = active && ctx.has_audio;

    Menu {
        title: "Transport".to_string(),
        entries: vec![
            item(
                "Load Audio File…",
                MenuCommand::Transport(TransportCommand::LoadAudio),
                active,
            ),
            keyed(
                if ctx.is_playing { "Pause" } else { "Play" },
                PLAY_PAUSE,
                MenuCommand::Transport(TransportCommand::PlayPause),
                loaded,
            ),
            checkable(
                "Bypass Effect",
                MenuCommand::Transport(TransportCommand::Bypass),
                loaded,
                ctx.is_bypassed,
            ),
            MenuEntry::Separator,
            item(
                "View Signal Graph",
                MenuCommand::Transport(TransportCommand::ViewGraph),
                loaded,
            ),
        ],
    }
}

fn window_menu(ctx: &MenuContext, surface: MenuSurface) -> Menu {
    let mut entries = Vec::new();

    // Minimize and Zoom are the OS's, and off macOS the title bar already offers
    // both — so the per-window bar is just the window list.
    if surface == MenuSurface::Global {
        entries.push(predefined(
            "Minimize",
            Some(MINIMIZE),
            PredefinedItem::Minimize,
        ));
        entries.push(predefined("Zoom", None, PredefinedItem::Zoom));
        entries.push(MenuEntry::Separator);
    }

    let mut window = |id: WindowId, enabled: bool| {
        entries.push(checkable(
            id.menu_label(),
            MenuCommand::FocusWindow(id),
            enabled,
            ctx.focused == id,
        ));
    };

    window(WindowId::Editor, ctx.has_project);
    window(WindowId::Simulator, ctx.has_project);
    // Opening the Graph on nothing would render four empty plots, so it waits
    // for audio — unless it is already open, in which case focusing it is fine.
    window(WindowId::Graph, ctx.graph_open || ctx.has_audio);
    // DOOM.666 is an easter egg. The Window menu lists it only once the user has
    // found it for themselves; advertising it here would give the joke away.
    if ctx.doom_open {
        window(WindowId::Doom, true);
    }

    if surface == MenuSurface::Global {
        entries.push(MenuEntry::Separator);
        entries.push(predefined(
            "Bring All to Front",
            None,
            PredefinedItem::BringAllToFront,
        ));
    }

    Menu {
        title: "Window".to_string(),
        entries,
    }
}

fn help_menu() -> Menu {
    Menu {
        title: "Help".to_string(),
        entries: vec![item("HeadroomLab Help", MenuCommand::Help, true)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An editor with one clean, editable, commentable buffer open.
    fn editing() -> MenuContext {
        MenuContext {
            focused: WindowId::Editor,
            has_project: true,
            has_open_tab: true,
            active_tab_editable: true,
            can_comment: true,
            code_area_focused: true,
            ..MenuContext::default()
        }
    }

    fn global(ctx: &MenuContext) -> MenuModel {
        MenuModel::build(ctx, MenuSurface::Global)
    }

    #[test]
    fn save_follows_the_dirty_flags() {
        let clean = global(&editing());
        assert!(!clean.is_enabled(MenuCommand::Save));
        assert!(!clean.is_enabled(MenuCommand::SaveAll));

        let dirty = global(&MenuContext {
            active_tab_dirty: true,
            any_tab_dirty: true,
            ..editing()
        });
        assert!(dirty.is_enabled(MenuCommand::Save));
        assert!(dirty.is_enabled(MenuCommand::SaveAll));
    }

    #[test]
    fn save_all_can_be_enabled_while_save_is_not() {
        // A dirty buffer in a background tab: Save acts on the active one, which
        // has nothing to write.
        let model = global(&MenuContext {
            any_tab_dirty: true,
            ..editing()
        });
        assert!(!model.is_enabled(MenuCommand::Save));
        assert!(model.is_enabled(MenuCommand::SaveAll));
    }

    #[test]
    fn edit_commands_are_disabled_unless_the_editor_is_focused() {
        for focused in [
            WindowId::Splash,
            WindowId::Initial,
            WindowId::Simulator,
            WindowId::Graph,
            WindowId::Doom,
        ] {
            let model = global(&MenuContext {
                focused,
                has_selection: true,
                can_paste: true,
                ..editing()
            });
            for command in [
                EditorCommand::Undo,
                EditorCommand::Redo,
                EditorCommand::Cut,
                EditorCommand::Copy,
                EditorCommand::Paste,
                EditorCommand::SelectAll,
                EditorCommand::SelectLine,
                EditorCommand::SelectNextOccurrence,
                EditorCommand::ToggleComment,
                EditorCommand::DuplicateLine,
                EditorCommand::DeleteLine,
            ] {
                assert!(
                    !model.is_enabled(MenuCommand::Edit(command)),
                    "{command:?} enabled while {focused:?} had focus"
                );
            }
            assert!(!model.is_enabled(MenuCommand::Find));
        }
    }

    #[test]
    fn doom_greys_out_everything_but_quit_and_close_window() {
        let model = global(&MenuContext {
            focused: WindowId::Doom,
            doom_open: true,
            any_tab_dirty: true,
            active_tab_dirty: true,
            ..editing()
        });

        assert!(!model.is_enabled(MenuCommand::NewProject));
        assert!(!model.is_enabled(MenuCommand::OpenProject));
        assert!(!model.is_enabled(MenuCommand::NewFile));
        assert!(!model.is_enabled(MenuCommand::Save));
        assert!(!model.is_enabled(MenuCommand::SaveAll));
        assert!(!model.is_enabled(MenuCommand::CloseTab));
        assert!(!model.is_enabled(MenuCommand::BuildRun));
        assert!(!model.is_enabled(MenuCommand::Compile));
        assert!(!model.is_enabled(MenuCommand::OpenEmulator));

        assert!(model.is_enabled(MenuCommand::Quit));
        assert!(model.is_enabled(MenuCommand::CloseWindow));
    }

    #[test]
    fn splash_greys_out_the_file_menu() {
        let model = global(&MenuContext::default());
        assert!(!model.is_enabled(MenuCommand::NewProject));
        assert!(!model.is_enabled(MenuCommand::OpenProject));
        assert!(model.is_enabled(MenuCommand::Quit));
        assert!(model.is_enabled(MenuCommand::CloseWindow));
    }

    #[test]
    fn cut_and_copy_need_a_selection_and_paste_needs_a_clipboard() {
        let bare = global(&editing());
        assert!(!bare.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
        assert!(!bare.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
        assert!(!bare.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));

        let ready = global(&MenuContext {
            has_selection: true,
            can_paste: true,
            ..editing()
        });
        assert!(ready.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
        assert!(ready.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
        assert!(ready.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
    }

    #[test]
    fn a_read_only_buffer_can_be_copied_but_not_rewritten() {
        let model = global(&MenuContext {
            active_tab_editable: false,
            has_selection: true,
            can_paste: true,
            ..editing()
        });
        assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
        assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::SelectAll)));
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::DeleteLine)));
    }

    /// On macOS an enabled item's accelerator is consumed by the menu before the
    /// window sees the key, so the Edit menu may only be live when the code area
    /// is what the keystroke would otherwise reach. Anything else — the terminal,
    /// the find bar, an inline rename — must get its own `⌘C` / `⌘V` / `⌘A`.
    #[test]
    fn the_edit_menu_stands_down_unless_the_code_area_has_focus() {
        let model = global(&MenuContext {
            code_area_focused: false,
            has_selection: true,
            can_paste: true,
            ..editing()
        });
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::SelectAll)));
        assert!(!model.is_enabled(MenuCommand::Find));

        // Saving is deliberately not focus-dependent: "save my work" must not
        // depend on where the caret happens to be.
        let dirty = global(&MenuContext {
            code_area_focused: false,
            active_tab_dirty: true,
            ..editing()
        });
        assert!(dirty.is_enabled(MenuCommand::Save));
    }

    /// D5: ⌘W closes the tab in the Editor and the window everywhere else, so
    /// the two items never claim the chord at the same time.
    #[test]
    fn close_window_takes_cmd_w_wherever_there_are_no_tabs() {
        let editor = global(&editing());
        assert_eq!(
            editor.find(MenuCommand::CloseTab).unwrap().shortcut,
            Some(CLOSE_TAB)
        );
        assert_eq!(
            editor.find(MenuCommand::CloseWindow).unwrap().shortcut,
            Some(CLOSE_WINDOW)
        );

        for focused in [WindowId::Simulator, WindowId::Graph, WindowId::Doom] {
            let model = global(&MenuContext {
                focused,
                ..editing()
            });
            let close_tab = model.find(MenuCommand::CloseTab).unwrap();
            let close_window = model.find(MenuCommand::CloseWindow).unwrap();
            assert!(!close_tab.enabled, "{focused:?} has no tabs to close");
            assert_eq!(close_window.shortcut, Some(CLOSE_TAB));
            assert!(close_window.enabled);
        }
    }

    #[test]
    fn toggle_comment_needs_a_language_with_line_comments() {
        let model = global(&MenuContext {
            can_comment: false,
            ..editing()
        });
        assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::ToggleComment)));
    }

    #[test]
    fn open_recent_is_empty_without_recents_and_capped_by_the_caller() {
        let none = global(&editing());
        assert!(none.find(MenuCommand::OpenRecent(0)).is_none());
        assert!(none.find(MenuCommand::ClearRecents).is_none());

        let ctx = MenuContext {
            recents: vec!["A".into(), "B".into(), "C".into()],
            ..editing()
        };
        let some = global(&ctx);
        assert!(some.is_enabled(MenuCommand::OpenRecent(2)));
        assert!(some.find(MenuCommand::OpenRecent(3)).is_none());
        assert!(some.is_enabled(MenuCommand::ClearRecents));
    }

    #[test]
    fn doom_is_absent_from_the_window_menu_until_it_is_open() {
        let hidden = global(&editing());
        assert!(
            hidden
                .find(MenuCommand::FocusWindow(WindowId::Doom))
                .is_none()
        );

        let shown = global(&MenuContext {
            doom_open: true,
            ..editing()
        });
        assert!(shown.is_enabled(MenuCommand::FocusWindow(WindowId::Doom)));
    }

    #[test]
    fn the_focused_window_is_ticked() {
        let model = global(&editing());
        let editor = model
            .find(MenuCommand::FocusWindow(WindowId::Editor))
            .unwrap();
        let simulator = model
            .find(MenuCommand::FocusWindow(WindowId::Simulator))
            .unwrap();
        assert_eq!(editor.checked, Some(true));
        assert_eq!(simulator.checked, Some(false));
    }

    #[test]
    fn the_graph_waits_for_audio_unless_it_is_already_open() {
        let quiet = global(&editing());
        assert!(!quiet.is_enabled(MenuCommand::FocusWindow(WindowId::Graph)));

        let loaded = global(&MenuContext {
            has_audio: true,
            ..editing()
        });
        assert!(loaded.is_enabled(MenuCommand::FocusWindow(WindowId::Graph)));

        let already = global(&MenuContext {
            graph_open: true,
            ..editing()
        });
        assert!(already.is_enabled(MenuCommand::FocusWindow(WindowId::Graph)));
    }

    #[test]
    fn a_running_build_disables_both_build_actions() {
        let model = global(&MenuContext {
            build_running: true,
            ..editing()
        });
        assert!(!model.is_enabled(MenuCommand::BuildRun));
        assert!(!model.is_enabled(MenuCommand::Compile));
        // Opening the emulator on the previously built library is still fine.
        assert!(model.is_enabled(MenuCommand::OpenEmulator));
    }

    #[test]
    fn transport_needs_the_simulator_focused_and_audio_loaded() {
        let unfocused = global(&MenuContext {
            has_audio: true,
            ..editing()
        });
        assert!(!unfocused.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));

        let silent = global(&MenuContext {
            focused: WindowId::Simulator,
            ..editing()
        });
        assert!(silent.is_enabled(MenuCommand::Transport(TransportCommand::LoadAudio)));
        assert!(!silent.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));

        let loaded = global(&MenuContext {
            focused: WindowId::Simulator,
            has_audio: true,
            ..editing()
        });
        assert!(loaded.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));
        assert!(loaded.is_enabled(MenuCommand::Transport(TransportCommand::ViewGraph)));
    }

    #[test]
    fn play_pause_follows_the_playing_flag_and_bypass_is_checkable() {
        let ctx = MenuContext {
            focused: WindowId::Simulator,
            has_audio: true,
            ..editing()
        };
        let stopped = global(&ctx);
        assert_eq!(
            stopped
                .find(MenuCommand::Transport(TransportCommand::PlayPause))
                .unwrap()
                .label,
            "Play"
        );

        let playing = global(&MenuContext {
            is_playing: true,
            is_bypassed: true,
            ..ctx
        });
        let item = playing
            .find(MenuCommand::Transport(TransportCommand::PlayPause))
            .unwrap();
        assert_eq!(item.label, "Pause");
        assert_eq!(
            playing
                .find(MenuCommand::Transport(TransportCommand::Bypass))
                .unwrap()
                .checked,
            Some(true)
        );
    }

    /// D4: the one bar macOS shares between every window must never reorder or
    /// drop a title as focus moves — only enablement may change.
    #[test]
    fn global_menu_titles_never_change() {
        let expected = vec![
            "HeadroomLab",
            "File",
            "Edit",
            "Build",
            "Transport",
            "Window",
            "Help",
        ];
        for focused in [
            WindowId::Splash,
            WindowId::Initial,
            WindowId::Editor,
            WindowId::Simulator,
            WindowId::Graph,
            WindowId::Doom,
        ] {
            for has_project in [false, true] {
                let model = global(&MenuContext {
                    focused,
                    has_project,
                    ..MenuContext::default()
                });
                assert_eq!(model.titles(), expected, "titles moved for {focused:?}");
            }
        }
    }

    /// D14: a per-window bar carries only what that window can act on.
    #[test]
    fn per_window_titles_match_the_spec() {
        let ctx = editing();
        let titles = |window| {
            MenuModel::build(&ctx, MenuSurface::Window(window))
                .titles()
                .join(" ")
        };

        assert_eq!(titles(WindowId::Editor), "File Edit Build Window Help");
        assert_eq!(titles(WindowId::Simulator), "File Transport Window Help");
        assert_eq!(titles(WindowId::Graph), "File Window Help");
        assert_eq!(titles(WindowId::Splash), "");
        assert_eq!(titles(WindowId::Initial), "");
        assert_eq!(titles(WindowId::Doom), "");
    }

    /// The two surfaces must agree, or the platforms drift. Built from the same
    /// context, any command present in both is enabled in both or neither.
    #[test]
    fn the_two_surfaces_agree_on_enablement() {
        let ctx = MenuContext {
            has_selection: true,
            can_paste: true,
            any_tab_dirty: true,
            active_tab_dirty: true,
            ..editing()
        };
        let global = MenuModel::build(&ctx, MenuSurface::Global);
        let per_window = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Editor));

        for command in [
            MenuCommand::NewProject,
            MenuCommand::OpenProject,
            MenuCommand::NewFile,
            MenuCommand::NewFolder,
            MenuCommand::Save,
            MenuCommand::SaveAll,
            MenuCommand::CloseTab,
            MenuCommand::CloseWindow,
            MenuCommand::Find,
            MenuCommand::BuildRun,
            MenuCommand::Compile,
            MenuCommand::OpenEmulator,
            MenuCommand::Quit,
            MenuCommand::About,
            MenuCommand::Help,
            MenuCommand::Edit(EditorCommand::Cut),
            MenuCommand::Edit(EditorCommand::Paste),
            MenuCommand::FocusWindow(WindowId::Simulator),
        ] {
            assert_eq!(
                global.is_enabled(command),
                per_window.is_enabled(command),
                "{command:?} differs between surfaces"
            );
        }
    }

    /// The per-window bar is drawn *inside* its window, so it must act as though
    /// that window has focus even when the OS says another one does.
    #[test]
    fn a_per_window_bar_assumes_its_own_window_is_focused() {
        let ctx = MenuContext {
            focused: WindowId::Editor,
            has_audio: true,
            ..editing()
        };
        let simulator = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Simulator));
        assert!(simulator.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));
    }

    #[test]
    fn the_app_menu_is_folded_into_file_off_macos() {
        let ctx = editing();
        let per_window = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Editor));
        assert!(!per_window.titles().contains(&"HeadroomLab"));
        assert!(per_window.is_enabled(MenuCommand::About));
        assert!(per_window.is_enabled(MenuCommand::Quit));
    }

    #[test]
    fn chords_render_for_the_host_platform() {
        if cfg!(target_os = "macos") {
            assert_eq!(SAVE.to_string(), "⌘S");
            assert_eq!(SAVE_ALL.to_string(), "⇧⌘S");
            assert_eq!(NEW_FOLDER.to_string(), "⇧⌥⌘N");
            assert_eq!(DUPLICATE_LINE.to_string(), "⇧⌥↓");
            assert_eq!(PLAY_PAUSE.to_string(), "Space");
        } else {
            assert_eq!(SAVE.to_string(), "Ctrl+S");
            assert_eq!(SAVE_ALL.to_string(), "Ctrl+Shift+S");
            assert_eq!(NEW_FOLDER.to_string(), "Ctrl+Shift+Alt+N");
            assert_eq!(DUPLICATE_LINE.to_string(), "Shift+Alt+Down");
            assert_eq!(PLAY_PAUSE.to_string(), "Space");
        }
    }
}
