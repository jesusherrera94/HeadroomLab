use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::domain::diagnostics::{Diagnostic, MAX_LISTED, Severity};
use crate::presentation::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jump {
    pub file: String,
    pub line: u32,
    pub column: Option<u32>,
}

pub fn problems_strip(
    ui: &mut egui::Ui,
    diagnostics: &[Diagnostic],
    is_stale: &dyn Fn(&str) -> bool,
) -> Option<Jump> {
    if diagnostics.is_empty() {
        return None;
    }

    let mut jump = None;

    egui::Frame::new()
        .fill(theme::DIALOG_BACKGROUND)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt("problems")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for diagnostic in diagnostics.iter().take(MAX_LISTED) {
                        if let Some(clicked) = row(ui, diagnostic, is_stale) {
                            jump = Some(clicked);
                        }
                    }

                    let hidden = diagnostics.len().saturating_sub(MAX_LISTED);
                    if hidden > 0 {
                        ui.label(
                            RichText::new(format!("+ {hidden} more — see the Build tab"))
                                .font(theme::small_font())
                                .color(theme::MUTED_ON_DARK),
                        );
                    }
                });
        });

    jump
}

fn row(
    ui: &mut egui::Ui,
    diagnostic: &Diagnostic,
    is_stale: &dyn Fn(&str) -> bool,
) -> Option<Jump> {
    let stale = diagnostic.file.as_deref().is_some_and(is_stale);

    let base = match diagnostic.severity {
        Severity::Error => theme::DIAGNOSTIC_ERROR,
        Severity::Warning => theme::DIAGNOSTIC_WARNING,
    };
    let color = if stale {
        base.gamma_multiply(0.45)
    } else {
        base
    };

    let icon = match diagnostic.severity {
        Severity::Error => ph::WARNING_CIRCLE,
        Severity::Warning => ph::WARNING,
    };

    let mut text = format!("{icon}  {}", diagnostic.severity.label());
    if let Some(location) = diagnostic.location() {
        text.push_str(&format!(" · {location}"));
    }
    text.push_str(&format!("  —  {}", diagnostic.message));
    if let Some(note) = diagnostic.notes.first() {
        text.push_str(&format!("  ({note})"));
    }

    let response = ui.add(
        egui::Label::new(RichText::new(text).font(theme::small_font()).color(color))
            .sense(egui::Sense::click())
            .truncate(),
    );

    // Only a diagnostic with a position can be jumped to; a `make: ***` failure
    // has nowhere to go, so it does not pretend to be clickable.
    let jumpable = diagnostic.has_position() && !stale;
    if !jumpable {
        return None;
    }

    if response.hovered() {
        ui.output_mut(|out| out.cursor_icon = egui::CursorIcon::PointingHand);
    }
    let response = response.on_hover_text("Go to this line");

    response.clicked().then(|| Jump {
        file: diagnostic.file.clone().unwrap_or_default(),
        line: diagnostic.line.unwrap_or(1),
        column: diagnostic.column,
    })
}

#[cfg(test)]
mod tests {
    use crate::domain::diagnostics::parse_diagnostics;

    #[test]
    fn a_jump_carries_what_the_compiler_reported() {
        let diagnostic = &parse_diagnostics("src/effect.cpp:14:24: error: boom")[0];
        assert!(diagnostic.has_position());
        assert_eq!(diagnostic.file.as_deref(), Some("src/effect.cpp"));
        assert_eq!(diagnostic.line, Some(14));
        assert_eq!(diagnostic.column, Some(24));
    }

    #[test]
    fn a_tool_failure_offers_nowhere_to_jump() {
        let diagnostic = &parse_diagnostics("make: *** [build/lib.dylib] Error 1")[0];
        assert!(
            !diagnostic.has_position(),
            "a row with no file must not look clickable"
        );
    }
}
