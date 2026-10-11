//! CPU rasteriser for the draw list. The Miyoo Mini has no usable GPU, so this
//! stands in for slot-gfx's GLES2 compositor on both desktop and device.

use crate::draw::{Canvas, Draw, DrawProfile, Face, TexId, TextureKind, Textures};
use std::io;
use std::sync::{Arc, RwLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

struct Texture {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    kind: TextureKind,
}

pub struct Soft {
    pub w: u32,
    pub h: u32,
    /// 0x00RRGGBB, row-major, `w * h`.
    pub fb: Vec<u32>,
    scale: f32,
    clear_colour: u32,
    textures: Arc<RwLock<Vec<Texture>>>,
    profile: DrawProfile,
    worker: Option<RenderWorker>,
}

struct RenderWorker {
    send: Option<mpsc::SyncSender<RenderJob>>,
    receive: mpsc::Receiver<RenderResult>,
    thread: Option<JoinHandle<()>>,
    buffer: Option<Vec<u32>>,
}

struct RenderJob {
    draws: Arc<[Draw]>,
    buffer: Vec<u32>,
    width: u32,
    full_height: u32,
    first_row: u32,
    scale: f32,
}

struct RenderResult {
    buffer: Vec<u32>,
    profile: DrawProfile,
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        self.send.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Textures for Soft {
    fn create_texture(&mut self, face: &Face, kind: TextureKind) -> TexId {
        let mut textures = self.textures.write().expect("texture lock poisoned");
        textures.push(Texture {
            w: face.w,
            h: face.h,
            rgba: premultiply(&face.rgba),
            kind,
        });
        TexId(textures.len() - 1)
    }
}

impl Soft {
    pub fn new(panel_w: u32, panel_h: u32, canvas: Canvas) -> Self {
        Self::new_inner(
            panel_w,
            panel_h,
            canvas,
            Arc::new(RwLock::new(Vec::new())),
            None,
        )
    }

    /// Create a CPU renderer with a persistent worker for the lower half of
    /// the display. The device frontend uses this to occupy both CPU cores.
    pub fn new_threaded(panel_w: u32, panel_h: u32, canvas: Canvas) -> io::Result<Self> {
        let (send, jobs) = mpsc::sync_channel::<RenderJob>(1);
        let (results, receive) = mpsc::sync_channel::<RenderResult>(1);
        let textures = Arc::new(RwLock::new(Vec::new()));
        let worker_textures = Arc::clone(&textures);
        let thread = thread::Builder::new()
            .name("slot-raster".into())
            .spawn(move || {
                while let Ok(mut job) = jobs.recv() {
                    let texture_guard = worker_textures.read().expect("texture lock poisoned");
                    let profile = render_region(
                        &mut job.buffer,
                        job.width,
                        job.full_height,
                        job.first_row,
                        job.scale,
                        &job.draws,
                        &texture_guard,
                    );
                    drop(texture_guard);
                    if results
                        .send(RenderResult {
                            buffer: job.buffer,
                            profile,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self::new_inner(
            panel_w,
            panel_h,
            canvas,
            textures,
            Some(RenderWorker {
                send: Some(send),
                receive,
                thread: Some(thread),
                buffer: Some(vec![0; (panel_w * (panel_h - panel_h / 2)) as usize]),
            }),
        ))
    }

    fn new_inner(
        panel_w: u32,
        panel_h: u32,
        canvas: Canvas,
        textures: Arc<RwLock<Vec<Texture>>>,
        worker: Option<RenderWorker>,
    ) -> Self {
        Soft {
            w: panel_w,
            h: panel_h,
            fb: vec![0; (panel_w * panel_h) as usize],
            scale: panel_w as f32 / canvas.w,
            clear_colour: 0,
            textures,
            profile: DrawProfile::default(),
            worker,
        }
    }

    pub fn clear(&mut self, rgb: u32) {
        self.fb.fill(rgb);
        self.clear_colour = rgb;
        if let Some(buffer) = self
            .worker
            .as_mut()
            .and_then(|worker| worker.buffer.as_mut())
        {
            buffer.fill(rgb);
        }
    }

    pub fn reset_draw_profile(&mut self) {
        self.profile = DrawProfile::default();
    }

    pub fn take_draw_profile(&mut self) -> DrawProfile {
        std::mem::take(&mut self.profile)
    }

    pub fn draw(&mut self, items: &[Draw]) {
        if let Some(worker) = &mut self.worker {
            let split_row = self.h / 2;
            let split = split_row as usize * self.w as usize;
            let draws: Arc<[Draw]> = Arc::from(items);
            let buffer = worker
                .buffer
                .take()
                .unwrap_or_else(|| vec![self.clear_colour; self.fb.len() - split]);
            let job = RenderJob {
                draws,
                buffer,
                width: self.w,
                full_height: self.h,
                first_row: split_row,
                scale: self.scale,
            };
            let sent = worker
                .send
                .as_ref()
                .is_some_and(|sender| sender.send(job).is_ok());
            if sent {
                let texture_guard = self.textures.read().expect("texture lock poisoned");
                let top_profile = render_region(
                    &mut self.fb[..split],
                    self.w,
                    self.h,
                    0,
                    self.scale,
                    items,
                    &texture_guard,
                );
                match worker.receive.recv() {
                    Ok(result) => {
                        self.fb[split..].copy_from_slice(&result.buffer);
                        worker.buffer = Some(result.buffer);
                        self.profile = combine_profiles(top_profile, result.profile);
                        return;
                    }
                    Err(_) => {
                        // The upper half is already complete. Finish the lower
                        // half on this thread if the worker stopped unexpectedly.
                        self.profile = combine_profiles(
                            top_profile,
                            render_region(
                                &mut self.fb[split..],
                                self.w,
                                self.h,
                                split_row,
                                self.scale,
                                items,
                                &texture_guard,
                            ),
                        );
                        return;
                    }
                }
            }
            // Worker failure is unusual; render synchronously so the UI can
            // remain usable instead of exiting to MainUI.
            let texture_guard = self.textures.read().expect("texture lock poisoned");
            self.profile = render_region(
                &mut self.fb,
                self.w,
                self.h,
                0,
                self.scale,
                items,
                &texture_guard,
            );
            return;
        }

        let texture_guard = self.textures.read().expect("texture lock poisoned");
        self.profile = render_region(
            &mut self.fb,
            self.w,
            self.h,
            0,
            self.scale,
            items,
            &texture_guard,
        );
    }
}

fn combine_profiles(mut a: DrawProfile, b: DrawProfile) -> DrawProfile {
    for i in 0..a.micros.len() {
        a.micros[i] = a.micros[i].saturating_add(b.micros[i]);
        a.calls[i] = a.calls[i].saturating_add(b.calls[i]);
    }
    a
}

fn render_region(
    fb: &mut [u32],
    width: u32,
    full_height: u32,
    first_row: u32,
    scale: f32,
    items: &[Draw],
    textures: &[Texture],
) -> DrawProfile {
    let mut profile = DrawProfile::default();
    for item in items {
        let start = Instant::now();
        let kind = match *item {
            Draw::Rect { x, y, w, h, colour } => {
                raster_rect(fb, width, full_height, first_row, scale, x, y, w, h, colour);
                0
            }
            Draw::Tex {
                x,
                y,
                w,
                h,
                tex,
                alpha,
            } => {
                if let Some(texture) = textures.get(tex.0) {
                    raster_texture(
                        fb,
                        width,
                        full_height,
                        first_row,
                        scale,
                        x,
                        y,
                        w,
                        h,
                        texture,
                        alpha,
                    );
                    match texture.kind {
                        TextureKind::Cart => 1,
                        TextureKind::Shadow => 2,
                        TextureKind::Chrome => 3,
                        TextureKind::Ui => 4,
                        TextureKind::Other => 5,
                    }
                } else {
                    5
                }
            }
        };
        profile.micros[kind] = profile.micros[kind]
            .saturating_add(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
        profile.calls[kind] += 1;
    }
    profile
}

fn span(a: f32, b: f32, limit: u32) -> Option<(u32, u32)> {
    let first = (a - 0.5).ceil().max(0.0) as u32;
    let end = ((b - 0.5).ceil().max(0.0) as u32).min(limit);
    (first < end).then_some((first, end - 1))
}

fn raster_rect(
    fb: &mut [u32],
    width: u32,
    full_height: u32,
    first_row: u32,
    scale: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    colour: [f32; 4],
) {
    let Some((x0, x1)) = span(x * scale, (x + w) * scale, width) else {
        return;
    };
    let Some((mut y0, mut y1)) = span(y * scale, (y + h) * scale, full_height) else {
        return;
    };
    y0 = y0.max(first_row);
    y1 = y1.min(first_row + (fb.len() / width as usize) as u32 - 1);
    if y0 > y1 {
        return;
    }
    let alpha = (colour[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let source = [
        (colour[0].clamp(0.0, 1.0) * f32::from(alpha) + 0.5) as u8,
        (colour[1].clamp(0.0, 1.0) * f32::from(alpha) + 0.5) as u8,
        (colour[2].clamp(0.0, 1.0) * f32::from(alpha) + 0.5) as u8,
        alpha,
    ];
    if alpha == 0 {
        return;
    }
    for py in y0..=y1 {
        let row = (py - first_row) as usize * width as usize;
        for px in x0..=x1 {
            composite(&mut fb[row + px as usize], source, 256);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn raster_texture(
    fb: &mut [u32],
    width: u32,
    full_height: u32,
    first_row: u32,
    scale: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    t: &Texture,
    alpha: f32,
) {
    let (dx, dy, dw, dh) = (x * scale, y * scale, w * scale, h * scale);
    let Some((x0, x1)) = span(dx, dx + dw, width) else {
        return;
    };
    let Some((mut y0, mut y1)) = span(dy, dy + dh, full_height) else {
        return;
    };
    y0 = y0.max(first_row);
    y1 = y1.min(first_row + (fb.len() / width as usize) as u32 - 1);
    if y0 > y1 || alpha <= 0.0 || t.w == 0 || t.h == 0 || dw <= 0.0 || dh <= 0.0 {
        return;
    }
    let opacity = (alpha.clamp(0.0, 1.0) * 256.0 + 0.5) as u16;
    if t.kind == TextureKind::Chrome
        && (dx - dx.round()).abs() < 0.001
        && (dy - dy.round()).abs() < 0.001
        && (dw - t.w as f32).abs() < 0.001
        && (dh - t.h as f32).abs() < 0.001
    {
        blit_exact(
            fb,
            width as usize,
            first_row,
            t,
            x0,
            y0,
            x1,
            y1,
            dx.round() as i32,
            dy.round() as i32,
            opacity,
        );
        return;
    }
    let xs = axis_samples(x0, x1, dx, t.w as usize, dw);
    let ys = axis_samples(y0, y1, dy, t.h as usize, dh);
    for (row_index, &(ylo, yhi, fy)) in ys.iter().enumerate() {
        let py = y0 as usize + row_index;
        let row = (py - first_row as usize) * width as usize;
        let top_lo = ylo * t.w as usize * 4;
        let top_hi = yhi * t.w as usize * 4;
        for (column_index, &(xlo, xhi, fx)) in xs.iter().enumerate() {
            let px = x0 as usize + column_index;
            let sample = sample_bilinear(
                t,
                top_lo + xlo * 4,
                top_lo + xhi * 4,
                top_hi + xlo * 4,
                top_hi + xhi * 4,
                fx,
                fy,
            );
            composite(&mut fb[row + px], sample, opacity);
        }
    }
}

fn blit_exact(
    fb: &mut [u32],
    fb_width: usize,
    first_row: u32,
    t: &Texture,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    left: i32,
    top: i32,
    opacity: u16,
) {
    for py in y0..=y1 {
        let sy = py as i32 - top;
        let row = (py - first_row) as usize * fb_width;
        for px in x0..=x1 {
            let sx = px as i32 - left;
            let src = (sy as usize * t.w as usize + sx as usize) * 4;
            composite(
                &mut fb[row + px as usize],
                [
                    t.rgba[src],
                    t.rgba[src + 1],
                    t.rgba[src + 2],
                    t.rgba[src + 3],
                ],
                opacity,
            );
        }
    }
}

fn premultiply(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for pixel in out.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    out
}

type AxisSample = (usize, usize, u16);

fn axis_samples(
    first: u32,
    last: u32,
    origin: f32,
    source_len: usize,
    dest_len: f32,
) -> Vec<AxisSample> {
    let scale = source_len as f32 / dest_len;
    (first..=last)
        .map(|pixel| {
            let source =
                ((pixel as f32 + 0.5 - origin) * scale - 0.5).clamp(0.0, (source_len - 1) as f32);
            let lo = source as usize;
            let hi = (lo + 1).min(source_len - 1);
            let frac = ((source - lo as f32) * 256.0 + 0.5) as u16;
            (lo, hi, frac.min(256))
        })
        .collect()
}

fn lerp8(a: u8, b: u8, fraction: u16) -> u8 {
    (((u32::from(a) * (256 - u32::from(fraction))) + u32::from(b) * u32::from(fraction) + 128) >> 8)
        as u8
}

fn sample_bilinear(
    t: &Texture,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    fx: u16,
    fy: u16,
) -> [u8; 4] {
    let mut out = [0; 4];
    for k in 0..4 {
        let top = lerp8(t.rgba[a + k], t.rgba[b + k], fx);
        let bottom = lerp8(t.rgba[c + k], t.rgba[d + k], fx);
        out[k] = lerp8(top, bottom, fy);
    }
    out
}

fn composite(dst: &mut u32, src: [u8; 4], opacity: u16) {
    let alpha = (u32::from(src[3]) * u32::from(opacity) + 128) >> 8;
    if alpha == 0 {
        return;
    }
    if alpha == 255 {
        *dst = (u32::from(src[0]) << 16) | (u32::from(src[1]) << 8) | u32::from(src[2]);
        return;
    }
    let inverse = 255 - alpha;
    let scale = |premul: u8, shift: u32| {
        let source = (u32::from(premul) * u32::from(opacity) + 128) >> 8;
        let target = (*dst >> shift) & 0xff;
        source + (target * inverse + 127) / 255
    };
    *dst = (scale(src[0], 16) << 16) | (scale(src[1], 8) << 8) | scale(src[2], 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strips that share fractional edges must cover every pixel exactly once.
    #[test]
    fn abutting_rects_leave_no_seams() {
        let canvas = Canvas { w: 720.0, h: 540.0 };
        let mut soft = Soft::new(640, 480, canvas);
        soft.clear(0);
        let mut x = 100.0;
        while x < 300.0 {
            soft.draw(&[Draw::Rect {
                x,
                y: 10.0,
                w: 1.0,
                h: 5.0,
                colour: [1.0, 1.0, 1.0, 0.5],
            }]);
            x += 1.0;
        }
        let row = (12 * 640) as usize;
        let lit: Vec<u32> = soft.fb[row + 100..row + 260].to_vec();
        assert!(lit.iter().all(|&p| p == lit[0]), "uneven strip coverage");
    }
}
