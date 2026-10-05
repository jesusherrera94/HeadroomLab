use eframe::egui;

#[derive(Default)]
pub struct TransportEvents {
    pub upload_clicked: bool,
    pub play_toggled: bool,
    pub bypass_toggled: bool,
    pub view_graph_clicked: bool,
    pub seek_to: Option<f32>,
}

pub fn transport_bar(
    ui: &mut egui::Ui,
    has_audio: bool,
    is_playing: bool,
    is_bypassed: bool,
    current_time: &mut f32,
    duration: f32,
) -> TransportEvents {
    let mut events = TransportEvents::default();

    ui.horizontal(|ui| {
        if ui.button("Upload Audio").clicked() {
            events.upload_clicked = true;
        }
        let play_label = if is_playing { "Stop" } else { "Play" };
        if ui
            .add_enabled(has_audio, egui::Button::new(play_label))
            .clicked()
        {
            events.play_toggled = true;
        }
        let bypass_label = if is_bypassed {
            "Bypassed [ON]"
        } else {
            "Bypass [OFF]"
        };
        if ui
            .add_enabled(has_audio, egui::Button::new(bypass_label))
            .clicked()
        {
            events.bypass_toggled = true;
        }
        if ui
            .add_enabled(has_audio, egui::Button::new("View graph"))
            .clicked()
        {
            events.view_graph_clicked = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label(format!("{}s", current_time.round() as i64));
        ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(50.0);
        let slider = egui::Slider::new(current_time, 0.0..=duration.max(0.0)).show_value(false);
        if ui.add_enabled(has_audio, slider).changed() {
            events.seek_to = Some(*current_time);
        }
        ui.label(format!("{}s", duration.round() as i64));
    });

    events
}
