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

pub struct NativeMenu {
    _menu: Menu,
    commands: HashMap<MenuId, MenuCommand>,
    built_from: Option<MenuContext>,
}

impl NativeMenu {
    pub fn install(ctx: &MenuContext) -> Self {
        let mut menu = Self {
            _menu: Menu::new(),
            commands: HashMap::new(),
            built_from: None,
        };
        menu.rebuild(ctx);
        menu
    }

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
            if let Err(e) = menu.append(&submenu) {
                eprintln!("native menu: could not append {:?}: {e}", top.title);
                return;
            }
        }

        menu.init_for_nsapp();
        self._menu = menu;
        self.commands = commands;
        self.built_from = Some(ctx.clone());
    }

    pub fn poll(&self) -> Vec<MenuCommand> {
        let mut commands = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
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
    }
}

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
        "Space" => return None,
        other => {
            debug_assert!(false, "no muda Code for chord key {other:?}");
            return None;
        }
    })
}
