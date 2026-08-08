//! The in-window menu bar, drawn on Windows and Linux.
//!
//! macOS gets a real `NSMenu` instead (`presentation::native_menu`); this is the
//! other half of **D1**. Both render the same [`MenuModel`], so the two
//! platforms cannot offer different commands or grey out different things — the
//! only differences are the ones D14 and the cross-platform table call for, and
//! those are decided when the model is *built*, not here.
//!
//! Compiled on every platform so it stays type-checked and testable on the
//! machine this is developed on; only `app_controller` decides whether to draw
//! it.

use eframe::egui;

use crate::domain::menu::{MenuCommand, MenuEntry, MenuItem, MenuModel};

/// The tick on a checked item, and the blank that keeps unchecked ones aligned
/// with it. egui has no checkable menu item, so the mark is part of the label.
const CHECK_MARK: &str = "✓";
const CHECK_GUTTER: &str = "\u{2007}";

/// Draws the bar and returns the command the user picked, if any.
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
                // An empty submenu (Open Recent with no history) is shown but
                // disabled: hiding it would shuffle the items below it, and the
                // point of the row is to say the history exists and is empty.
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
    // Predefined items are the OS's own (Minimize, Hide, Services…). The model
    // never puts one on a per-window surface, so reaching one here would mean a
    // macOS-only entry leaked into the egui bar — skip rather than draw a row
    // that could not do anything.
    if matches!(item.command, MenuCommand::Predefined(_)) {
        return;
    }

    // A checkmark column, laid out so ticked and unticked rows align. egui has no
    // checkable menu item, so the glyph is part of the label.
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
