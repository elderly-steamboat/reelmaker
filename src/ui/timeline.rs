//! Timeline widget: preset chips, thumbnail track, draggable reel window, playhead, scrubbing.

use crate::model::Preset;
use crate::state::AppState;
use crate::ui::geometry::{time_to_x, window_start_from_pointer, x_to_time};
use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke, StrokeKind};

const ACCENT: Color32 = Color32::from_rgb(255, 61, 113);

fn preset_label(p: Preset) -> String {
    let name = match p {
        Preset::Short => "Short",
        Preset::Medium => "Medium",
        Preset::Long => "Long",
    };
    format!("{name} · {}s", p.seconds() as u32)
}

fn tag(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Draws the timeline. Returns a time to seek to when the user clicked/dragged the track.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, playhead: f64) -> Option<f64> {
    let duration = state.media.as_ref().map(|m| m.info.duration).unwrap_or(0.0);
    let mut seek = None;

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("REEL WINDOW").small().weak());
            for p in Preset::ALL {
                if ui.selectable_label(state.preset == p, preset_label(p)).clicked() {
                    state.set_preset(p);
                }
            }
            if let Some(w) = state.window {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("{:.0}s reel", w.length));
                });
            }
        });

        let (rect, track) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, Color32::from_gray(28));

        // thumbnails
        if let Some(m) = &state.media {
            let n = m.thumbs.len().max(1);
            let cell = rect.width() / n as f32;
            for (i, path) in m.thumbs.iter().enumerate() {
                let r = Rect::from_min_size(
                    egui::pos2(rect.left() + cell * i as f32, rect.top()),
                    egui::vec2(cell.ceil(), rect.height()),
                );
                let img = egui::Image::new(format!("file://{}", path.display())).tint(Color32::from_white_alpha(150));
                img.paint_at(ui, r);
            }
        }

        if duration > 0.0 {
            if track.clicked() || track.dragged() {
                if let Some(pos) = track.interact_pointer_pos() {
                    seek = Some(x_to_time(pos.x - rect.left(), duration, rect.width()));
                }
            }

            if let Some(w) = state.window {
                let x0 = rect.left() + time_to_x(w.start, duration, rect.width());
                let x1 = rect.left() + time_to_x(w.end(), duration, rect.width());
                let win = Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom()));
                let id = ui.id().with("reel-window");
                let resp = ui.interact(win, id, Sense::drag());
                if resp.drag_started() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let grab = x_to_time(pos.x - rect.left(), duration, rect.width()) - w.start;
                        ui.memory_mut(|m| m.data.insert_temp(id, grab));
                    }
                }
                if resp.dragged() {
                    let grab: f64 = ui.memory(|m| m.data.get_temp(id)).unwrap_or(0.0);
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let start = window_start_from_pointer(pos.x - rect.left(), grab, duration, rect.width());
                        if let Some(w) = &mut state.window {
                            w.drag_to(duration, start);
                        }
                    }
                }
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                painter.rect_filled(win, 6.0, Color32::from_rgba_unmultiplied(255, 61, 113, 56));
                painter.rect_stroke(win, 6.0, Stroke::new(2.0_f32, ACCENT), StrokeKind::Inside);
                painter.text(
                    win.left_top() + egui::vec2(6.0, 4.0),
                    egui::Align2::LEFT_TOP,
                    format!("{} – {}", tag(w.start), tag(w.end())),
                    egui::FontId::proportional(11.0),
                    Color32::WHITE,
                );
            }

            let hx = rect.left() + time_to_x(playhead, duration, rect.width());
            painter.line_segment([egui::pos2(hx, rect.top()), egui::pos2(hx, rect.bottom())], Stroke::new(2.0_f32, Color32::WHITE));
        }

        ui.horizontal(|ui| {
            for i in 0..=5 {
                if i > 0 {
                    ui.add_space((ui.available_width() / (6 - i) as f32 - 28.0).max(0.0));
                }
                ui.weak(tag(duration * i as f64 / 5.0));
            }
        });
    });
    seek
}
