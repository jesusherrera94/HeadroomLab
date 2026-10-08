mod builders;
mod chord;
mod context;

use crate::domain::editing::EditorCommand;

use builders::{
    app_menu, build_menu, edit_menu, file_menu, help_menu, transport_menu, window_menu,
};
pub use chord::*;
pub use context::MenuContext;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSurface {
    Global,
    Window(WindowId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredefinedItem {
    Services,
    Hide,
    HideOthers,
    ShowAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCommand {
    LoadAudio,
    PlayPause,
    Bypass,
    ViewGraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    About,
    CheckForUpdates,
    Help,
    Quit,

    NewProject,
    OpenProject,
    OpenRecent(usize),
    ClearRecents,

    NewFile,
    NewFolder,
    Save,
    SaveAll,
    CloseTab,
    CloseWindow,

    Edit(EditorCommand),
    Find,

    OpenEmulator,
    BuildRun,
    Compile,

    Transport(TransportCommand),

    FocusWindow(WindowId),

    Predefined(PredefinedItem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<Chord>,
    pub command: MenuCommand,
    pub enabled: bool,
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

impl MenuModel {
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
                app_menu(&ctx),
                file_menu(&ctx, surface),
                edit_menu(&ctx),
                build_menu(&ctx),
                transport_menu(&ctx),
                window_menu(&ctx),
                help_menu(&ctx, surface),
            ],
            MenuSurface::Window(WindowId::Editor) => vec![
                file_menu(&ctx, surface),
                edit_menu(&ctx),
                build_menu(&ctx),
                window_menu(&ctx),
                help_menu(&ctx, surface),
            ],
            MenuSurface::Window(WindowId::Simulator) => vec![
                file_menu(&ctx, surface),
                transport_menu(&ctx),
                window_menu(&ctx),
                help_menu(&ctx, surface),
            ],
            MenuSurface::Window(WindowId::Graph) => {
                vec![
                    file_menu(&ctx, surface),
                    window_menu(&ctx),
                    help_menu(&ctx, surface),
                ]
            }
            MenuSurface::Window(_) => Vec::new(),
        };

        Self { menus }
    }

    pub fn titles(&self) -> Vec<&str> {
        self.menus.iter().map(|m| m.title.as_str()).collect()
    }

    pub fn find(&self, command: MenuCommand) -> Option<&MenuItem> {
        self.menus.iter().find_map(|menu| find_in(menu, command))
    }

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

#[cfg(test)]
mod tests;
