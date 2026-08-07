//! The macOS native menu bar (D1).
//!
//! Renders the very same [`MenuModel`] the egui bar does, so the two platforms
//! cannot offer different commands — see
//! `presentation::components::organisms::menu_bar` for the other half.
//!
//! Two rules govern this file:
//!
//! * **Predefined items only where the OS's own behaviour is what we want.**
//!   `PredefinedMenuItem::quit()` calls `NSApp terminate:`, which would walk
//!   straight past the unsaved-work confirmation, and the copy/cut/paste ones go
//!   through the responder chain, which an egui-painted window never answers.
//!   Anything with a consequence is a custom item routed through our own
//!   dispatch (D8).
//! * **Menu events are never acted on where they are received.** [`poll`] only
//!   reports them; the app controller decides when it is safe to open a window,
//!   because creating a viewport is only legal on a frame that has eframe's
//!   event-loop thread-local set (see `app_controller::prepare_simulator`).
//!
//! The whole bar is rebuilt whenever the context changes rather than diffed item
//! by item: `setMainMenu:` swaps it atomically, context changes are human-paced,
//! and a rebuild cannot drift out of sync with the model the way a diff can.

use std::collections::HashMap;

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{
    CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::Modifiers as Mods,
};

use crate::domain::menu::{
    Chord, MenuCommand, MenuContext, MenuEntry, MenuItem as ModelItem, MenuModel, MenuSurface,
    PredefinedItem,
};

/// The installed bar, plus the map from the ids muda hands back to the commands
/// they stand for. Rebuilding replaces both together, so a stale id can only
/// fail to resolve — it can never resolve to the wrong command.
pub struct NativeMenu {
    /// Kept alive: dropping it would take the `NSMenu` with it.
    _menu: Menu,
    commands: HashMap<MenuId, MenuCommand>,
    /// The context the current bar was built from. `None` until the first sync.
    built_from: Option<MenuContext>,
}

impl NativeMenu {
    /// Builds the bar for `ctx` and makes it the application's main menu.
    pub fn install(ctx: &MenuContext) -> Self {
        let mut menu = Self {
            _menu: Menu::new(),
            commands: HashMap::new(),
            built_from: None,
        };
        menu.rebuild(ctx);
        menu
    }

    /// Rebuilds the bar if — and only if — anything it depends on changed (S6).
    /// At the 100 ms repaint tick an unconditional rebuild would be thousands of
    /// pointless `NSMenu` allocations an hour.
    pub fn sync(&mut self, ctx: &MenuContext) {
        if self.built_from.as_ref() == Some(ctx) {
            return;
        }
        self.rebuild(ctx);
    }

    fn rebuild(&mut self, ctx: &MenuContext) {
        let model = MenuModel::build(ctx, MenuSurface::Global);
        let mut commands = HashMap::new();
        let menu = Menu::new();

        for top in &model.menus {
            let submenu = Submenu::new(&top.title, true);
            append_entries(&submenu, &top.entries, &mut commands);
            // A failure here means the menu could not be assembled at all;
            // there is nothing to fall back to, so it is reported and the bar is
            // left as whatever was there before.
            if let Err(e) = menu.append(&submenu) {
                eprintln!("native menu: could not append {:?}: {e}", top.title);
                return;
            }
        }

        // Install the new bar *before* dropping the old one: `setMainMenu:`
        // retains it, so the outgoing menu is only released once it is no longer
        // the app's.
        menu.init_for_nsapp();
        self._menu = menu;
        self.commands = commands;
        self.built_from = Some(ctx.clone());
    }

    /// Drains muda's queue, returning the commands clicked since the last call.
    ///
    /// Deliberately returns them rather than acting: the caller decides *where*
    /// in the frame each one runs.
    pub fn poll(&self) -> Vec<MenuCommand> {
        let mut commands = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            // A miss means the id came from a bar that has since been rebuilt:
            // the click was for a menu that no longer exists, so it is dropped.
            if let Some(command) = self.commands.get(event.id()) {
                commands.push(*command);
            }
        }
        commands
    }
}

fn append_entries(
    parent: &Submenu,
    entries: &[MenuEntry],
    commands: &mut HashMap<MenuId, MenuCommand>,
) {
    for entry in entries {
        let result = match entry {
            MenuEntry::Separator => parent.append(&PredefinedMenuItem::separator()),

            MenuEntry::Item(item) => match item.command {
                MenuCommand::Predefined(which) => parent.append(&predefined(which, item)),
                _ => append_custom(parent, item, commands),
            },

            MenuEntry::Submenu(sub) => {
                // An empty Open Recent is shown and disabled rather than hidden:
                // removing it would shuffle everything below it as the history
                // comes and goes.
                let child = Submenu::new(&sub.title, !sub.entries.is_empty());
                append_entries(&child, &sub.entries, commands);
                parent.append(&child)
            }
        };

        if let Err(e) = result {
            eprintln!("native menu: could not append an item: {e}");
        }
    }
}

/// Appends a custom item, recording the id muda assigns so the click can be
/// resolved back to its command.
fn append_custom(
    parent: &Submenu,
    item: &ModelItem,
    commands: &mut HashMap<MenuId, MenuCommand>,
) -> muda::Result<()> {
    let accelerator = item.shortcut.and_then(accelerator_for);

    match item.checked {
        Some(checked) => {
            let native = CheckMenuItem::new(&item.label, item.enabled, checked, accelerator);
            commands.insert(native.id().clone(), item.command);
            parent.append(&native)
        }
        None => {
            let native = MenuItem::new(&item.label, item.enabled, accelerator);
            commands.insert(native.id().clone(), item.command);
            parent.append(&native)
        }
    }
}

fn predefined(which: PredefinedItem, item: &ModelItem) -> PredefinedMenuItem {
    let text = Some(item.label.as_str());
    match which {
        PredefinedItem::Services => PredefinedMenuItem::services(text),
        PredefinedItem::Hide => PredefinedMenuItem::hide(text),
        PredefinedItem::HideOthers => PredefinedMenuItem::hide_others(text),
        PredefinedItem::ShowAll => PredefinedMenuItem::show_all(text),
        PredefinedItem::Minimize => PredefinedMenuItem::minimize(text),
        // macOS's "Zoom" is the green-button behaviour, which is `maximize`.
        PredefinedItem::Zoom => PredefinedMenuItem::maximize(text),
        PredefinedItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(text),
    }
}

/// The muda accelerator for one of our chords.
///
/// `Modifiers::SUPER` is what muda turns into `NSEventModifierFlags::Command`
/// (`platform_impl/macos/accelerator.rs:65`), so `command` maps there rather
/// than to `CONTROL`.
fn accelerator_for(chord: Chord) -> Option<Accelerator> {
    let mut mods = Mods::empty();
    if chord.command {
        mods |= Modifiers::SUPER;
    }
    if chord.shift {
        mods |= Modifiers::SHIFT;
    }
    if chord.alt {
        mods |= Modifiers::ALT;
    }

    let code = code_for(chord.key)?;
    Some(Accelerator::new(Some(mods), code))
}

fn code_for(key: &str) -> Option<Code> {
    Some(match key {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "F" => Code::KeyF,
        "H" => Code::KeyH,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Z" => Code::KeyZ,
        "/" => Code::Slash,
        "Down" => Code::ArrowDown,
        "Up" => Code::ArrowUp,
        // Space belongs to the Simulator's transport, which is an egui shortcut
        // in its own window — binding it here would swallow the space bar
        // application-wide, including in the code editor.
        "Space" => return None,
        other => {
            debug_assert!(false, "no muda Code for chord key {other:?}");
            return None;
        }
    })
}
