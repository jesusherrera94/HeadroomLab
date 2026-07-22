//! Initial window: header (logo + title + subtitle), Create/Open actions, and
//! the Recent-projects list. The Create modal renders on top when open.

use eframe::egui::{self, RichText};

use crate::domain::project::RecentProject;
use crate::presentation::components::atoms::logo;
use crate::presentation::components::molecules::create_project_modal::{
    CreateModalEvents, CreateModalFields, create_project_modal,
};
use crate::presentation::components::molecules::recent_project_item::recent_project_item;
use crate::presentation::initial_controller::InitialState;
use crate::presentation::theme;

#[derive(Default)]
pub struct InitialViewEvents {
    pub create_clicked: bool,
    pub open_clicked: bool,
    pub recent_clicked: Option<usize>,
    pub modal: CreateModalEvents,
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut InitialState,
    recents: &[RecentProject],
) -> InitialViewEvents {
    let mut events = InitialViewEvents::default();

    egui::CentralPanel::default().show(ui, |ui| {
        ui.add_space(8.0);

        // Header: logo tile + title + subtitle.
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            logo::logo_tile(ui, 44.0);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Headroom Lab")
                        .font(theme::title_font())
                        .color(theme::LABEL_ON_DARK),
                );
                ui.label(
                    RichText::new("Audio DSP code editor")
                        .font(theme::body_font())
                        .color(theme::MUTED_ON_DARK),
                );
            });
        });

        ui.add_space(16.0);

        // Actions row.
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let full = ui.available_width() - 8.0;
            let btn = egui::vec2((full - 10.0) / 2.0, 34.0);
            if ui
                .add_sized(btn, theme::selectable_button("Create project", true))
                .clicked()
            {
                events.create_clicked = true;
            }
            if ui
                .add_sized(btn, theme::selectable_button("Open project…", false))
                .clicked()
            {
                events.open_clicked = true;
            }
        });

        ui.add_space(16.0);

        // Recent section.
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new("RECENT")
                    .font(theme::small_font())
                    .color(theme::MUTED_ON_DARK),
            );
        });
        ui.add_space(4.0);

        if recents.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("No recent projects")
                        .font(theme::body_font())
                        .color(theme::MUTED_ON_DARK),
                );
            });
        } else {
            for (index, project) in recents.iter().enumerate() {
                if recent_project_item(ui, project) {
                    events.recent_clicked = Some(index);
                }
            }
        }
    });

    // Create modal on top of the window.
    if state.modal_open {
        events.modal = create_project_modal(
            &ui.ctx().clone(),
            CreateModalFields {
                name: &mut state.name,
                path: &mut state.path,
                path_edited: &mut state.path_edited,
            },
        );
    }

    events
}
