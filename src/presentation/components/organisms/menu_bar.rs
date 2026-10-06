use eframe::egui;

use crate::domain::menu::{MenuCommand, MenuEntry, MenuItem, MenuModel};

const CHECK_MARK: &str = "✓";
const CHECK_GUTTER: &str = "\u{2007}";

pub fn menu_bar(ui: &mut egui::Ui, model: &MenuModel) -> Option<MenuCommand> {
    let mut chosen = None;

    egui::MenuBar::new().ui(ui, |ui| {
        ui.add_space(4.0);
        for menu in &model.menus {
            ui.menu_button(&menu.title, |ui| {
                ui.set_min_width(220.0);
                render_entries(ui, &menu.entries, &mut chosen);
            });
        }
    });

    chosen
}

fn render_entries(ui: &mut egui::Ui, entries: &[MenuEntry], chosen: &mut Option<MenuCommand>) {
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                ui.separator();
            }
            MenuEntry::Item(item) => render_item(ui, item, chosen),
            MenuEntry::Submenu(sub) => {
                if sub.entries.is_empty() {
                    ui.add_enabled(false, egui::Button::new(&sub.title).frame(false));
                } else {
                    ui.menu_button(&sub.title, |ui| {
                        ui.set_min_width(220.0);
                        render_entries(ui, &sub.entries, chosen);
                    });
                }
            }
        }
    }
}

fn render_item(ui: &mut egui::Ui, item: &MenuItem, chosen: &mut Option<MenuCommand>) {
    if matches!(item.command, MenuCommand::Predefined(_)) {
        return;
    }

    let label = match item.checked {
        Some(true) => format!("{}  {}", CHECK_MARK, item.label),
        Some(false) => format!("{}  {}", CHECK_GUTTER, item.label),
        None => item.label.clone(),
    };

    let mut button = egui::Button::new(label).frame(false);
    if let Some(chord) = item.shortcut {
        button = button.shortcut_text(chord.to_string());
    }

    if ui.add_enabled(item.enabled, button).clicked() {
        *chosen = Some(item.command);
        ui.close();
    }
}
