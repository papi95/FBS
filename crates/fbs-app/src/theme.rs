//! Dark console palette. Broadcast rooms are dim; the only saturated colors are
//! tallies and meters, so the eye lands on program/preview state first.
//!
//! Only long-stable egui style fields are touched here. Corner radius and frame
//! helpers changed names between egui releases, so panels draw their own edges.

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(13, 14, 18);
pub const PANEL: Color32 = Color32::from_rgb(18, 20, 25);
pub const PANEL_2: Color32 = Color32::from_rgb(23, 26, 32);
pub const PANEL_3: Color32 = Color32::from_rgb(31, 35, 43);
pub const LINE: Color32 = Color32::from_rgb(44, 49, 59);
pub const TEXT: Color32 = Color32::from_rgb(226, 229, 236);
pub const MUTED: Color32 = Color32::from_rgb(128, 137, 152);
pub const AMBER: Color32 = Color32::from_rgb(240, 178, 68);
pub const TALLY_RED: Color32 = Color32::from_rgb(228, 74, 78);
pub const TALLY_GREEN: Color32 = Color32::from_rgb(96, 200, 138);

pub(crate) fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();

    let v = &mut style.visuals;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = PANEL_2;
    v.override_text_color = Some(TEXT);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.inactive.bg_fill = PANEL_3;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.hovered.bg_fill = Color32::from_rgb(41, 46, 56);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    v.widgets.active.bg_fill = Color32::from_rgb(52, 58, 70);
    v.selection.bg_fill = Color32::from_rgb(96, 74, 30);
    v.selection.stroke = Stroke::new(1.0_f32, AMBER);

    style.spacing.item_spacing = egui::vec2(7.0, 5.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.slider_width = 150.0;

    style.text_styles.insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Button, FontId::new(12.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Small, FontId::new(10.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Monospace, FontId::new(11.0, FontFamily::Monospace));

    ctx.set_style(style);
}
