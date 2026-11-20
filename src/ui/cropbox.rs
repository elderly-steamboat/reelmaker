//! The 9:16 crop box drawn over the landscape stage in Zoom & crop mode.

use crate::state::AppState;
use crate::ui::geometry::{crop_width_fraction, crop_x_from_pointer};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, StrokeKind};

const ACCENT: Color32 = Color32::from_rgb(255, 61, 113);

/// Draws the crop box inside `stage` and lets the user drag it horizontally.
pub fn show(ui: &mut egui::Ui, stage: Rect, state: &mut AppState) {
    let Some(m) = &state.media else { return };
    let aspect = m.info.width as f64 / m.info.height as f64;
    let zoom = state.style.zoom;
    let frac = crop_width_fraction(aspect, zoom) as f32;
    let (bw, bh) = (stage.width() * frac, stage.height() / zoom as f32);

    // Same placement rule as the ffmpeg crop: x = clamp(cropX*iw - w/2, 0, iw - w), centred vertically.
    let left = (state.style.crop_x as f32 * stage.width() - bw / 2.0).clamp(0.0, stage.width() - bw);
    let top = (stage.height() - bh) / 2.0;
    let rect = Rect::from_min_size(stage.min + egui::vec2(left, top), egui::vec2(bw, bh));

    // dim everything outside the box
    let dim = Color32::from_black_alpha(140);
    let p = ui.painter_at(stage);
    p.rect_filled(Rect::from_min_max(stage.min, egui::pos2(rect.left(), stage.bottom())), 0.0, dim);
    p.rect_filled(Rect::from_min_max(egui::pos2(rect.right(), stage.top()), stage.max), 0.0, dim);
    p.rect_filled(Rect::from_min_max(egui::pos2(rect.left(), stage.top()), egui::pos2(rect.right(), rect.top())), 0.0, dim);
    p.rect_filled(Rect::from_min_max(egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), stage.bottom())), 0.0, dim);
    p.rect_stroke(rect, 0.0, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
    p.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        egui::Align2::CENTER_BOTTOM,
        "drag to reframe",
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );

    let resp = ui.interact(rect, ui.id().with("crop-box"), Sense::drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if resp.dragged() && stage.width() > 0.0 {
        let cx = state.style.crop_x + (resp.drag_delta().x / stage.width()) as f64;
        state.set_crop_x(cx);
    }
    // click anywhere else on the stage to centre the box there
    let bg = ui.interact(stage, ui.id().with("crop-bg"), Sense::click());
    if bg.clicked() {
        if let Some(pos) = bg.interact_pointer_pos() {
            state.set_crop_x(crop_x_from_pointer(pos.x, stage.left(), stage.width()));
        }
    }
}
