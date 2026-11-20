//! libmpv wrapper: playback, seeking, A-B looping and the live 9:16 filter graph.
//!
//! Two flavours: `new_headless` (vo=null, for tests) and `new_gl` (mpv's OpenGL render API, drawn
//! into an FBO by the UI). Rendering must only happen on the thread that owns the GL context.

use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Player {
    // Field order matters: the render context must drop before the Mpv handle it borrows.
    render: Option<RenderContext<'static>>,
    mpv: Box<Mpv>,
    redraw: Arc<AtomicBool>,
}

// SAFETY: `Mpv` is Send+Sync. `RenderContext` is only ever used from the GL/UI thread that created
// it; the Player is moved between threads only inside an `Arc<Mutex<_>>` that the UI thread locks
// for every render call.
unsafe impl Send for Player {}

/// Carries the GL loader through mpv's `'static` init-params slot. mpv only calls it while the
/// render context is being created, during which the borrowed closure is still alive.
struct GlLoader(*const (dyn Fn(&str) -> *const c_void + 'static));

fn gl_get_proc_address(ctx: &GlLoader, name: &str) -> *mut c_void {
    // SAFETY: see `GlLoader`.
    unsafe { (*ctx.0)(name) as *mut c_void }
}

fn err<E: std::fmt::Debug>(what: &str) -> impl FnOnce(E) -> String + '_ {
    move |e| format!("{what}: {e:?}")
}

impl Player {
    pub fn new_headless() -> Result<Player, String> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "null")?;
            init.set_property("ao", "null")?;
            init.set_property("keep-open", "yes")?;
            init.set_property("idle", "yes")?;
            Ok(())
        })
        .map_err(err("mpv init"))?;
        Ok(Player { render: None, mpv: Box::new(mpv), redraw: Arc::new(AtomicBool::new(false)) })
    }

    /// `get_proc_address` resolves GL functions (the UI passes eframe's loader).
    /// `audio=false` for the silent 9:16 preview instance.
    pub fn new_gl(get_proc_address: &dyn Fn(&str) -> *const c_void, audio: bool) -> Result<Player, String> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("keep-open", "yes")?;
            init.set_property("idle", "yes")?;
            init.set_property("hwdec", "auto-safe")?;
            if !audio {
                init.set_property("ao", "null")?;
                init.set_property("audio", "no")?;
            }
            Ok(())
        })
        .map_err(err("mpv init"))?;
        let mpv = Box::new(mpv);

        // SAFETY: erase the closure's lifetime; it is only called during create_render_context below.
        let loader = GlLoader(unsafe {
            std::mem::transmute::<*const (dyn Fn(&str) -> *const c_void + '_), *const (dyn Fn(&str) -> *const c_void + 'static)>(
                get_proc_address as *const _,
            )
        });
        let ctx = mpv
            .create_render_context([
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams { get_proc_address: gl_get_proc_address, ctx: loader }),
            ])
            .map_err(err("mpv render context"))?;
        // SAFETY: `mpv` is boxed (stable address) and outlives `render` (field drop order).
        let mut ctx: RenderContext<'static> = unsafe { std::mem::transmute(ctx) };

        let redraw = Arc::new(AtomicBool::new(true));
        let flag = redraw.clone();
        ctx.set_update_callback(move || flag.store(true, Ordering::SeqCst));
        Ok(Player { render: Some(ctx), mpv, redraw })
    }

    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        self.mpv
            .command("loadfile", &[&path.to_string_lossy(), "replace"])
            .map_err(err("loadfile"))
    }

    pub fn set_paused(&mut self, paused: bool) {
        let _ = self.mpv.set_property("pause", paused);
    }

    /// Absolute, frame-exact seek.
    pub fn seek(&mut self, secs: f64) {
        let _ = self.mpv.command("seek", &[&format!("{secs}"), "absolute+exact"]);
    }

    /// Current playback position in seconds (0.0 before a file is loaded).
    pub fn position(&self) -> f64 {
        self.mpv.get_property::<f64>("time-pos").unwrap_or(0.0)
    }

    /// True when paused, including when playback reached the end (`keep-open`).
    pub fn paused(&self) -> bool {
        self.mpv.get_property::<bool>("pause").unwrap_or(true)
    }

    /// Size of the frames mpv produces after the filter graph (`None` until video output is set up).
    pub fn video_out_size(&self) -> Option<(i64, i64)> {
        let w = self.mpv.get_property::<i64>("video-out-params/w").ok()?;
        let h = self.mpv.get_property::<i64>("video-out-params/h").ok()?;
        Some((w, h))
    }

    /// `None` until the file has loaded.
    pub fn duration(&self) -> Option<f64> {
        self.mpv.get_property::<f64>("duration").ok().filter(|d| *d > 0.0)
    }

    /// A-B loop between the two times; `None` clears it.
    pub fn set_loop(&mut self, window: Option<(f64, f64)>) {
        match window {
            Some((a, b)) => {
                let _ = self.mpv.set_property("ab-loop-a", a);
                let _ = self.mpv.set_property("ab-loop-b", b);
            }
            None => {
                let _ = self.mpv.set_property("ab-loop-a", "no");
                let _ = self.mpv.set_property("ab-loop-b", "no");
            }
        }
    }

    /// Applies (or clears) the 9:16 graph from `filters::reel_graph(.., "vid1", "vo")`.
    ///
    /// With a graph, mpv's own container-rotation handling is switched off (`video-rotate=no`):
    /// `preview_graph` already rotates the frames, and mpv would otherwise rotate the finished
    /// 9:16 canvas a second time. Without a graph, mpv's normal auto-rotation applies.
    pub fn set_reel_graph(&mut self, graph: Option<&str>) -> Result<(), String> {
        self.mpv
            .set_property("video-rotate", if graph.is_some() { "no" } else { "0" })
            .map_err(err("video-rotate"))?;
        self.mpv.set_property("lavfi-complex", graph.unwrap_or("")).map_err(err("lavfi-complex"))
    }

    /// Renders the current frame into `fbo` (0 = default framebuffer). GL thread only.
    pub fn render(&mut self, fbo: i32, w: u32, h: u32) {
        if let Some(r) = &self.render {
            let _ = r.render::<()>(fbo, w as i32, h as i32, true);
        }
        self.redraw.store(false, Ordering::SeqCst);
    }

    /// True when mpv has a new frame (or needs a repaint) since the last `render`.
    pub fn needs_redraw(&self) -> bool {
        self.redraw.load(Ordering::SeqCst)
    }
}
