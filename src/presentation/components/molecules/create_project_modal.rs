//! Create-project modal: Name + editable Path fields with inline validation
//! and Cancel/Create buttons. Presentation mirrors `error_dialog` for a
//! consistent modal look. Owns no state — the caller holds the buffers and
//! reacts to the returned events.

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::presentation::theme;

/// Mutable buffers + flags the modal reads and writes.
pub struct CreateModalFields<'a> {
    pub name: &'a mut String,
    pub path: &'a mut String,
    /// Set once the user manually edits Path, so the caller stops
    /// auto-syncing it from Name.
    pub path_edited: &'a mut bool,
}

#[derive(Default)]
pub struct CreateModalEvents {
    pub name_changed: bool,
    pub cancelled: bool,
    pub created: bool,
}

pub fn create_project_modal(ctx: &egui::Context, fields: CreateModalFields) -> CreateModalEvents {
    let mut events = CreateModalEvents::default();

    let name_valid = !fields.name.trim().is_empty();
    let path_valid = !fields.path.trim().is_empty();
    let valid = name_valid && path_valid;

    let frame = egui::Frame::new()
        .fill(theme::DIALOG_BACKGROUND)
        .stroke(Stroke::new(1.0, theme::INSET_BORDER))
        .corner_radius(CornerRadius::same(theme::DIALOG_CORNER_RADIUS))
        .inner_margin(20.0);

    egui::Modal::new(egui::Id::new("create_project_modal"))
        .backdrop_color(theme::MODAL_SCRIM)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.spacing_mut().item_spacing.y = 10.0;

            ui.label(
                RichText::new("Create project")
                    .font(theme::subtitle_font())
                    .strong()
                    .color(theme::LABEL_ON_DARK),
            );

            field_label(ui, "Name");
            let name_resp = ui.add(
                egui::TextEdit::singleline(fields.name)
                    .hint_text("My Project")
                    .desired_width(f32::INFINITY),
            );
            if name_resp.changed() {
                events.name_changed = true;
            }

            field_label(ui, "Path");
            let path_resp = ui.add(
                egui::TextEdit::singleline(fields.path)
                    .hint_text("Save location…")
                    .font(egui::FontId::monospace(theme::FONT_BODY))
                    .desired_width(f32::INFINITY),
            );
            if path_resp.changed() {
                *fields.path_edited = true;
            }

            // Inline validation message.
            if !valid {
                let msg = if !name_valid {
                    "Enter a project name."
                } else {
                    "Enter a save path."
                };
                ui.label(
                    RichText::new(msg)
                        .font(theme::small_font())
                        .color(theme::ERROR_COLOR),
                );
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    events.cancelled = true;
                }
                ui.add_space(4.0);
                let create = ui.add_enabled(valid, theme::selectable_button("Create", true));
                if create.clicked() {
                    events.created = true;
                }
            });
        });

    events
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::small_font())
            .color(theme::MUTED_ON_DARK),
    );
}
