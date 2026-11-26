//! Panel drawing. Kept out of `main.rs` so the app state machine stays readable.
//!
//! Edges are painted as nested filled rects rather than `rect_stroke`, because
//! the stroke signature changed in recent egui versions and this file must build
//! unchanged across them.

use crate::Studio;
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Vec2};
use fbs_core::director::TransitionKind;
use fbs_core::ids::SceneId;
use fbs_core::source::SourceKind;

/// Draw a bordered panel: outer fill acts as the 1px edge.
fn panel(painter: &egui::Painter, rect: Rect, fill: Color32, edge: Color32) {
    painter.rect_filled(rect, 3.0, edge);
    painter.rect_filled(rect.shrink(1.0), 2.0, fill);
}

pub(crate) fn top_bar(ui: &mut egui::Ui, s: &mut Studio) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new("FBS").strong().size(15.0).color(crate::theme::AMBER));
        ui.label(
            RichText::new("Free Broadcast Studio")
                .size(10.5)
                .color(crate::theme::MUTED),
        );
        ui.add_space(10.0);
        if ui.button("New scene").clicked() {
            s.new_scene();
        }
        if ui.button("Open").clicked() {
            s.open_project();
        }
        if ui.button("Save").clicked() {
            s.save_project();
        }
        ui.add_space(6.0);
        ui.label(RichText::new(&s.project.title).size(11.0).color(crate::theme::TEXT));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if s.recording { "■ Stop" } else { "● Record" };
            let color = if s.recording { crate::theme::TALLY_RED } else { crate::theme::TEXT };
            if ui
                .add(egui::Button::new(RichText::new(label).color(color)))
                .clicked()
            {
                s.recording = !s.recording;
                s.status = if s.recording {
                    "Record armed — encoder arrives in milestone 5".into()
                } else {
                    "Recording stopped".into()
                };
            }
            let canvas = s.project.canvas;
            ui.label(
                RichText::new(format!("{}×{} · {}p", canvas.width, canvas.height, s.project.fps))
                    .size(10.5)
                    .color(crate::theme::MUTED),
            );
        });
    });
}

pub(crate) fn status_bar(ui: &mut egui::Ui, s: &Studio) {
    ui.horizontal(|ui| {
        let tone = if s.director.is_live() { crate::theme::AMBER } else { crate::theme::MUTED };
        ui.label(RichText::new(&s.status).size(10.5).color(tone));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clip = s.mixer.is_clipping();
            ui.label(
                RichText::new(if clip { "MIX HOT" } else { "MIX OK" })
                    .size(10.5)
                    .color(if clip { crate::theme::TALLY_RED } else { crate::theme::MUTED }),
            );
            ui.label(
                RichText::new("Space = take · Esc = cut · drag T-bar for manual mix")
                    .size(10.5)
                    .color(crate::theme::MUTED),
            );
        });
    });
}

pub(crate) fn scene_list(ui: &mut egui::Ui, s: &mut Studio) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("SCENES").size(9.5).color(crate::theme::MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+").clicked() {
                s.new_scene();
            }
        });
    });
    ui.add_space(4.0);

    type Row = (SceneId, String, usize);
    let rows: Vec<Row> = s
        .director
        .graph
        .scenes
        .iter()
        .map(|sc| (sc.id, sc.name.clone(), sc.sources.len()))
        .collect();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (id, name, count) in rows {
                let is_live = id == s.director.program;
                let is_preview = s.director.preview == Some(id);

                let width = ui.available_width();
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::click());

                let fill = if is_live {
                    Color32::from_rgb(46, 22, 26)
                } else if is_preview {
                    Color32::from_rgb(20, 40, 30)
                } else {
                    crate::theme::PANEL_2
                };
                let edge = if is_live {
                    crate::theme::TALLY_RED
                } else if is_preview {
                    crate::theme::TALLY_GREEN
                } else {
                    crate::theme::LINE
                };

                let painter = ui.painter_at(rect);
                panel(&painter, rect, fill, edge);

                painter.text(
                    egui::pos2(rect.left() + 11.0, rect.top() + 8.0),
                    Align2::LEFT_TOP,
                    &name,
                    FontId::proportional(12.5),
                    crate::theme::TEXT,
                );
                painter.text(
                    egui::pos2(rect.left() + 11.0, rect.bottom() - 8.0),
                    Align2::LEFT_BOTTOM,
                    format!("{count} sources"),
                    FontId::proportional(9.5),
                    crate::theme::MUTED,
                );
                if is_live || is_preview {
                    let (tag, color) = if is_live {
                        ("PGM", crate::theme::TALLY_RED)
                    } else {
                        ("PVW", crate::theme::TALLY_GREEN)
                    };
                    painter.text(
                        egui::pos2(rect.right() - 10.0, rect.center().y),
                        Align2::RIGHT_CENTER,
                        tag,
                        FontId::monospace(9.0),
                        color,
                    );
                }

                if resp.clicked() {
                    s.set_preview(id);
                }
                if resp.double_clicked() {
                    s.cut(id);
                }
                ui.add_space(4.0);
            }
        });
}

pub(crate) fn monitors(ui: &mut egui::Ui, s: &mut Studio) {
    let avail = ui.available_size();
    let gap = 10.0;
    let bus_w = (avail.x - gap) * 0.5;
    let bus_h = (avail.y - 104.0).max(180.0);

    ui.horizontal(|ui| {
        bus_view(ui, "PROGRAM", s.director.program, crate::theme::TALLY_RED, bus_w, bus_h, s);
        ui.add_space(gap);
        bus_view(
            ui,
            "PREVIEW",
            s.director.preview.unwrap_or(s.director.program),
            crate::theme::TALLY_GREEN,
            bus_w,
            bus_h,
            s,
        );
    });

    ui.add_space(8.0);
    transition_row(ui, s);
    ui.add_space(6.0);
    source_row(ui, s);
}

/// Placeholder composition: draws the source stack as labelled blocks, so
/// transforms, opacity and stacking are visible before capture exists.
fn bus_view(
    ui: &mut egui::Ui,
    title: &str,
    scene: SceneId,
    tally: Color32,
    width: f32,
    height: f32,
    s: &Studio,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(9.5).color(tally));
            ui.label(
                RichText::new(
                    s.director
                        .graph
                        .get(scene)
                        .map(|x| x.name.clone())
                        .unwrap_or_default(),
                )
                .size(9.5)
                .color(crate::theme::MUTED),
            );
        });

        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, tally);
        painter.rect_filled(rect.shrink(2.0), 2.0, Color32::BLACK);

        let Some(scene) = s.director.graph.get(scene) else {
            return;
        };

        let inner = rect.shrink(2.0);
        let scale = (inner.width() / scene.width as f32).min(inner.height() / scene.height as f32);
        let origin = egui::pos2(
            inner.center().x - scene.width as f32 * scale * 0.5,
            inner.center().y - scene.height as f32 * scale * 0.5,
        );

        for src in scene.draw_list() {
            let r = src.transform.rect;
            let layer = Rect::from_min_size(
                egui::pos2(origin.x + r.x * scale, origin.y + r.y * scale),
                Vec2::new(r.w * scale, r.h * scale),
            );
            let base = match &src.kind {
                SourceKind::Color { rgba } => {
                    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
                }
                SourceKind::Camera { .. } => Color32::from_rgb(38, 44, 56),
                SourceKind::DesktopCapture { .. } => Color32::from_rgb(32, 40, 52),
                _ => crate::theme::PANEL_3,
            };
            let color = base.gamma_multiply(src.opacity.clamp(0.15, 1.0));
            painter.rect_filled(layer, 1.0, color);
            if layer.height() > 24.0 && layer.width() > 60.0 {
                painter.text(
                    layer.left_top() + Vec2::new(6.0, 4.0),
                    Align2::LEFT_TOP,
                    &src.name,
                    FontId::proportional(9.0),
                    crate::theme::MUTED,
                );
            }
        }

        painter.text(
            rect.left_bottom() + Vec2::new(8.0, -7.0),
            Align2::LEFT_BOTTOM,
            format!("{} sources", scene.sources.len()),
            FontId::monospace(8.5),
            crate::theme::MUTED,
        );
    });
}

fn transition_row(ui: &mut egui::Ui, s: &mut Studio) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("TRANSITION").size(9.5).color(crate::theme::MUTED));

        egui::ComboBox::from_id_salt("fx_kind")
            .width(96.0)
            .selected_text(kind_label(s.transition))
            .show_ui(ui, |ui| {
                for k in [
                    TransitionKind::Cut,
                    TransitionKind::Fade,
                    TransitionKind::SlideLeft,
                    TransitionKind::WipeDown,
                    TransitionKind::Stinger,
                ] {
                    if ui.selectable_label(s.transition == k, kind_label(k)).clicked() {
                        s.transition = k;
                        s.director.set_transition(k, s.transition_ms);
                    }
                }
            });

        let mut ms = s.transition_ms as f32;
        if ui
            .add(egui::Slider::new(&mut ms, 100.0..=2000.0).suffix(" ms").show_value(true))
            .changed()
        {
            s.transition_ms = ms as u32;
            s.director.set_transition(s.transition, s.transition_ms);
        }

        if ui.add(egui::Button::new(RichText::new("TAKE").strong())).clicked() {
            s.take();
        }
        if ui.button("Cut").clicked() {
            let program = s.director.program;
            s.cut(program);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(210.0, 18.0), Sense::drag());
            let mut pos = s.tbar;
            if resp.dragged() {
                if let Some(p) = ui.ctx().pointer_interact_pos() {
                    pos = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                }
            }
            if (pos - s.tbar).abs() > f32::EPSILON {
                s.tbar = pos;
                s.director.set_tbar(pos);
            }

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 3.0, crate::theme::PANEL_3);
            painter.rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(rect.width() * s.tbar, rect.height())),
                3.0,
                crate::theme::AMBER,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "T-BAR",
                FontId::monospace(8.5),
                crate::theme::MUTED,
            );
        });
    });
}

fn source_row(ui: &mut egui::Ui, s: &mut Studio) {
    let scene_id = s.director.preview.unwrap_or(s.director.program);
    let sources: Vec<(String, &'static str)> = s
        .director
        .graph
        .get(scene_id)
        .map(|sc| sc.sources.iter().map(|src| (src.name.clone(), kind_name(&src.kind))).collect())
        .unwrap_or_default();

    ui.horizontal_wrapped(|ui| {
        for (name, kind) in sources.iter().take(8) {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(112.0, 32.0), Sense::hover());
            let painter = ui.painter_at(rect);
            panel(&painter, rect, crate::theme::PANEL_2, crate::theme::LINE);
            painter.text(
                rect.left_top() + Vec2::new(7.0, 4.0),
                Align2::LEFT_TOP,
                name,
                FontId::proportional(10.0),
                crate::theme::TEXT,
            );
            painter.text(
                rect.left_bottom() + Vec2::new(7.0, -4.0),
                Align2::LEFT_BOTTOM,
                kind,
                FontId::proportional(8.5),
                crate::theme::MUTED,
            );
        }
    });
}

pub(crate) fn mixer_panel(ui: &mut egui::Ui, s: &mut Studio) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("MIXER").size(9.5).color(crate::theme::MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+ strip").clicked() {
                let n = s.mixer.strips.len() + 1;
                s.mixer.add(fbs_core::audio::Strip::new(
                    fbs_core::ids::SourceId::new(),
                    format!("Input {n}"),
                ));
            }
        });
    });
    ui.add_space(6.0);

    let count = s.mixer.strips.len();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 54.0).max(160.0))
        .show(ui, |ui| {
            for i in 0..count {
                let (name, gain_db, muted, solo, peak) = {
                    let st = &s.mixer.strips[i];
                    (st.name.clone(), st.gain_db(), st.muted, st.solo, st.peak)
                };

                let db_text = if gain_db.is_finite() {
                    format!("{gain_db:+.1} dB")
                } else {
                    "-inf".to_string()
                };

                // The frame sizes itself to its contents, so the slider and the
                // mute/solo buttons can never spill outside the strip.
                egui::Frame::none()
                    .fill(crate::theme::PANEL_2)
                    .stroke(egui::Stroke::new(1.0_f32, crate::theme::LINE))
                    .rounding(egui::Rounding::same(3.0_f32))
                    .inner_margin(egui::Margin::symmetric(7.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&name).size(11.0).color(crate::theme::TEXT));
                                ui.label(RichText::new(db_text).size(8.5).color(crate::theme::MUTED));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let (mrect, _) =
                                        ui.allocate_exact_size(Vec2::new(52.0, 6.0), Sense::hover());
                                    let p = ui.painter_at(mrect);
                                    p.rect_filled(mrect, 2.0, crate::theme::PANEL_3);
                                    let color = if peak > 0.92 {
                                        crate::theme::TALLY_RED
                                    } else if peak > 0.7 {
                                        crate::theme::AMBER
                                    } else {
                                        crate::theme::TALLY_GREEN
                                    };
                                    p.rect_filled(
                                        Rect::from_min_size(
                                            mrect.min,
                                            Vec2::new(
                                                mrect.width() * peak.clamp(0.0, 1.0),
                                                mrect.height(),
                                            ),
                                        ),
                                        2.0,
                                        color,
                                    );
                                },
                            );
                        });

                        let mut db = if gain_db.is_finite() { gain_db } else { -60.0 };
                        if ui
                            .add(
                                egui::Slider::new(&mut db, -60.0..=6.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            )
                            .changed()
                        {
                            s.mixer.strips[i].set_gain_db(db);
                        }

                        ui.horizontal(|ui| {
                            if ui.selectable_label(muted, "M").clicked() {
                                s.mixer.strips[i].muted = !muted;
                            }
                            if ui.selectable_label(solo, "S").clicked() {
                                s.mixer.strips[i].solo = !solo;
                            }
                            ui.label(
                                RichText::new(if muted { "muted" } else { "live" })
                                    .size(8.5)
                                    .color(if muted {
                                        crate::theme::MUTED
                                    } else {
                                        crate::theme::TALLY_GREEN
                                    }),
                            );
                        });
                    });

                ui.add_space(4.0);
            }
        });

    ui.add_space(4.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("MASTER").size(9.5).color(crate::theme::MUTED));
        let mut m = s.mixer.master;
        if ui
            .add(egui::Slider::new(&mut m, 0.0..=1.2).show_value(false).trailing_fill(true))
            .changed()
        {
            s.mixer.master = m;
            s.project.mixer.master = m;
        }
    });
}

fn kind_label(k: TransitionKind) -> &'static str {
    match k {
        TransitionKind::Cut => "Cut",
        TransitionKind::Fade => "Fade",
        TransitionKind::SlideLeft => "Slide",
        TransitionKind::WipeDown => "Wipe",
        TransitionKind::Stinger => "Stinger",
    }
}

fn kind_name(k: &SourceKind) -> &'static str {
    match k {
        SourceKind::Color { .. } => "color",
        SourceKind::Image { .. } => "image",
        SourceKind::DesktopCapture { .. } => "desktop",
        SourceKind::Camera { .. } => "camera",
        SourceKind::AudioInput { .. } => "audio",
        SourceKind::Text { .. } => "text",
        SourceKind::Browser { .. } => "browser",
        SourceKind::Scene { .. } => "nested scene",
        SourceKind::Media { .. } => "media",
    }
}
