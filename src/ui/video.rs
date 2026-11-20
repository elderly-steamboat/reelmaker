//! Draws a libmpv `Player` into an egui rect: mpv renders into an offscreen FBO, which is then
//! blitted into the rect on the default framebuffer (so mpv's viewport handling never fights egui's).

use crate::player::Player;
use eframe::egui::{self, Rect};
use eframe::{egui_glow, glow};
use glow::HasContext;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct VideoSurface {
    fbo: Option<glow::Framebuffer>,
    tex: Option<glow::Texture>,
    size: (i32, i32),
}

impl VideoSurface {
    fn ensure(&mut self, gl: &glow::Context, w: i32, h: i32) {
        if self.fbo.is_some() && self.size == (w, h) {
            return;
        }
        unsafe {
            if let Some(f) = self.fbo.take() {
                gl.delete_framebuffer(f);
            }
            if let Some(t) = self.tex.take() {
                gl.delete_texture(t);
            }
            let tex = gl.create_texture().expect("texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            let fbo = gl.create_framebuffer().expect("fbo");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.fbo = Some(fbo);
            self.tex = Some(tex);
            self.size = (w, h);
        }
    }
}

pub type SharedPlayer = Arc<Mutex<Player>>;
pub type SharedSurface = Arc<Mutex<VideoSurface>>;

/// Schedules the video to be painted into `rect` this frame.
pub fn paint_video(ui: &egui::Ui, rect: Rect, player: &SharedPlayer, surface: &SharedSurface) {
    let (player, surface) = (player.clone(), surface.clone());
    let cb = egui_glow::CallbackFn::new(move |info, painter| {
        let gl = painter.gl();
        let vp = info.viewport_in_pixels();
        let (w, h) = (vp.width_px, vp.height_px);
        if w <= 0 || h <= 0 {
            return;
        }
        let mut surf = surface.lock().unwrap();
        surf.ensure(gl, w, h);
        let fbo = surf.fbo.expect("fbo");
        player.lock().unwrap().render(fbo.0.get() as i32, w as u32, h as u32);
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, painter.intermediate_fbo());
            gl.blit_framebuffer(
                0, 0, w, h,
                vp.left_px, vp.from_bottom_px, vp.left_px + w, vp.from_bottom_px + h,
                glow::COLOR_BUFFER_BIT, glow::LINEAR,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, painter.intermediate_fbo());
        }
    });
    ui.painter().add(egui::PaintCallback { rect, callback: Arc::new(cb) });
}

/// Largest rect of the given width/height aspect that fits inside `avail`, centred.
pub fn fit_rect(avail: Rect, aspect: f32) -> Rect {
    if avail.width() <= 0.0 || avail.height() <= 0.0 {
        return avail;
    }
    let (w, h) = if avail.width() / avail.height() > aspect {
        (avail.height() * aspect, avail.height())
    } else {
        (avail.width(), avail.width() / aspect)
    };
    Rect::from_center_size(avail.center(), egui::vec2(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn fit_wide_into_tall_space() {
        let r = fit_rect(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 400.0)), 16.0 / 9.0);
        assert!((r.width() - 100.0).abs() < 1e-3 && (r.height() - 56.25).abs() < 1e-3);
        assert!((r.center().y - 200.0).abs() < 1e-3);
    }

    #[test]
    fn fit_tall_into_wide_space() {
        let r = fit_rect(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(400.0, 160.0)), 9.0 / 16.0);
        assert!((r.height() - 160.0).abs() < 1e-3 && (r.width() - 90.0).abs() < 1e-3);
    }
}
