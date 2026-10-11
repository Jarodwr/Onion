//! The SP's cartridge slot along the bottom edge and the cart's travel into
//! it (slot-ui/src/slot_chrome.rs), laid out against the canvas height.

use slot_store::Theme;
use slot_ui::ease;

use crate::draw::{rgb, Canvas, Draw, Face, TexId, TextureKind, Textures};
use crate::shelf::foot_y;

pub const MOUTH_H: f32 = 58.0;
/// slot's seated cart width (`slot_ui::seated_box(..).0`) plus clearance.
const SEATED_W: f32 = 240.0;
const MOUTH_W: f32 = SEATED_W + 14.0;
const SLIT_H: f32 = 9.0;
const LIP_H: f32 = 2.0;
const BAY_W: f32 = MOUTH_W + 18.0;
const SCOOP_W: f32 = MOUTH_W * 0.88;
const RECESS_H: f32 = 42.0;
const RIM_W: f32 = 2.0;
const SCOOP_FLAT: f32 = 4.0;
const CATCH_IN: f32 = 0.42;
const CATCH_OUT: f32 = 0.62;
const CREEP: f32 = 0.03;

struct Layout {
    w: f32,
    h: f32,
    cx: f32,
    band_y: f32,
    mouth_x: f32,
    bay_x: f32,
    bay_y: f32,
    slit_y: f32,
    scoop_y: f32,
    scoop_d: f32,
    seated_y: f32,
}

impl Layout {
    fn of(canvas: Canvas) -> Self {
        let band_y = canvas.h - MOUTH_H;
        let bay_y = band_y + LIP_H;
        let slit_y = bay_y + 5.0;
        let scoop_y = slit_y + SLIT_H;
        Layout {
            w: canvas.w,
            h: canvas.h,
            cx: canvas.w / 2.0,
            band_y,
            mouth_x: (canvas.w - MOUTH_W) / 2.0,
            bay_x: (canvas.w - BAY_W) / 2.0,
            bay_y,
            slit_y,
            scoop_y,
            scoop_d: RECESS_H - (scoop_y - bay_y),
            seated_y: bay_y + 4.0,
        }
    }
}

pub struct Colours {
    housing: [f32; 4],
    recess: [f32; 4],
    opening: [f32; 4],
    edge: [f32; 4],
}

impl From<&Theme> for Colours {
    fn from(t: &Theme) -> Self {
        Colours {
            housing: rgb(t.housing),
            recess: rgb(t.recess),
            opening: rgb(t.opening),
            edge: rgb(t.edge),
        }
    }
}

impl Default for Colours {
    fn default() -> Self {
        Colours::from(&Theme::default())
    }
}

pub struct SlotChrome<'a> {
    pub colours: &'a Colours,
    pub cached: Option<&'a ChromeTextures>,
    pub size: (f32, f32),
    pub seated: (f32, f32),
    pub face: Option<TexId>,
    pub colour: [f32; 4],
    /// Left edge and scale on the shelf, where travel starts.
    pub rest: f32,
    pub scale: f32,
    /// 0 on the shelf, 1 fully seated.
    pub seat: f32,
    /// Screen power; the slot fades out as the game screen comes up.
    pub screen: f32,
}

impl SlotChrome<'_> {
    pub fn draw(&self, canvas: Canvas, out: &mut Vec<Draw>) {
        let l = Layout::of(canvas);
        let c = self.colours;
        let seat = self.seat.clamp(0.0, 1.0);
        let chrome = 1.0 - self.screen.clamp(0.0, 1.0);

        if let Some(cached) = self.cached {
            cached.draw_back(chrome, out);
        } else {
            draw_back(&l, c, chrome, out);
        }

        let (cw, ch) = self.size;
        let scale = self.scale.clamp(0.0, 1.0);
        let (w0, h0) = (cw * scale, ch * scale);
        let (sw, sh) = self.seated;
        let (foot0, seated_foot) = (foot_y(canvas, ch), l.seated_y + sh);
        let catch = (l.band_y - foot0) / (seated_foot - foot0);
        let travel = travel(seat, catch);
        let k = (travel / catch).min(1.0);
        let (cw, ch) = (w0 + (sw - w0) * k, h0 + (sh - h0) * k);
        let centre = self.rest + w0 / 2.0;
        let x = centre + (l.cx - centre) * k - cw / 2.0;
        let y = foot0 + (seated_foot - foot0) * travel - ch;
        let alpha = if seat >= 1.0 { chrome } else { 1.0 };
        out.push(match self.face {
            Some(tex) => Draw::Tex {
                x,
                y,
                w: cw,
                h: ch,
                tex,
                alpha,
            },
            None => Draw::Rect {
                x,
                y,
                w: cw,
                h: ch,
                colour: [self.colour[0], self.colour[1], self.colour[2], alpha],
            },
        });

        if chrome < 1.0 {
            if let Some(cached) = self.cached {
                cached.draw_mask(out);
            } else {
                front_bands(&l, c, |x, y, w, h, _| {
                    out.push(Draw::Rect {
                        x,
                        y,
                        w,
                        h,
                        colour: [0.0, 0.0, 0.0, 1.0],
                    });
                });
            }
        }
        if let Some(cached) = self.cached {
            cached.draw_front(chrome, out);
        } else {
            draw_front(&l, c, chrome, out);
        }
    }
}

/// Supersampled, panel-sized versions of the slot's static layers.
pub struct ChromeTextures {
    back: TexId,
    front: TexId,
    mask: TexId,
    y: f32,
    h: f32,
}

impl ChromeTextures {
    pub fn new(
        canvas: Canvas,
        panel: (u32, u32),
        colours: &Colours,
        gfx: &mut dyn Textures,
    ) -> Self {
        let layout = Layout::of(canvas);
        let top = ((layout.band_y * panel.1 as f32 / canvas.h).floor() as u32).min(panel.1);
        let h = panel.1 - top;
        let layers = layer_draws(&layout, colours);
        let face = |draws: &[Draw]| rasterize(draws, canvas, panel, top);
        let back = gfx.create_texture(&face(&layers.0), TextureKind::Chrome);
        let front = gfx.create_texture(&face(&layers.1), TextureKind::Chrome);
        let mask = gfx.create_texture(&face(&layers.2), TextureKind::Chrome);
        let scale_y = panel.1 as f32 / canvas.h;
        ChromeTextures {
            back,
            front,
            mask,
            y: top as f32 / scale_y,
            h: h as f32 / scale_y,
        }
    }

    pub fn draw_empty(&self, out: &mut Vec<Draw>) {
        self.draw_back(1.0, out);
        self.draw_front(1.0, out);
    }

    fn draw_back(&self, alpha: f32, out: &mut Vec<Draw>) {
        self.push(self.back, alpha, out);
    }

    fn draw_front(&self, alpha: f32, out: &mut Vec<Draw>) {
        self.push(self.front, alpha, out);
    }

    fn draw_mask(&self, out: &mut Vec<Draw>) {
        self.push(self.mask, 1.0, out);
    }

    fn push(&self, tex: TexId, alpha: f32, out: &mut Vec<Draw>) {
        out.push(Draw::Tex {
            x: 0.0,
            y: self.y,
            w: crate::draw::CANVAS_W,
            h: self.h,
            tex,
            alpha,
        });
    }
}

pub fn prerender(
    canvas: Canvas,
    panel: (u32, u32),
    colours: &Colours,
    gfx: &mut dyn Textures,
) -> ChromeTextures {
    ChromeTextures::new(canvas, panel, colours, gfx)
}

fn layer_draws(l: &Layout, c: &Colours) -> (Vec<Draw>, Vec<Draw>, Vec<Draw>) {
    let mut back = Vec::new();
    draw_back(l, c, 1.0, &mut back);
    let mut front = Vec::new();
    draw_front(l, c, 1.0, &mut front);
    let mut mask = Vec::new();
    front_bands(l, c, |x, y, w, h, _| {
        mask.push(Draw::Rect {
            x,
            y,
            w,
            h,
            colour: [0.0, 0.0, 0.0, 1.0],
        });
    });
    (back, front, mask)
}

const SUPERSAMPLE: u32 = 4;

fn rasterize(draws: &[Draw], canvas: Canvas, panel: (u32, u32), top: u32) -> Face {
    let (w, h) = (panel.0, panel.1 - top);
    let (hi_w, hi_h) = (w * SUPERSAMPLE, h * SUPERSAMPLE);
    let (sx, sy) = (
        panel.0 as f32 * SUPERSAMPLE as f32 / canvas.w,
        panel.1 as f32 * SUPERSAMPLE as f32 / canvas.h,
    );
    let hi_top = top * SUPERSAMPLE;
    let mut hi = vec![0u8; (hi_w * hi_h * 4) as usize];

    for draw in draws {
        let Draw::Rect { x, y, w, h, colour } = *draw else {
            continue;
        };
        let Some((x0, x1)) = span(x * sx, (x + w) * sx, hi_w) else {
            continue;
        };
        let Some((y0, y1)) = span(y * sy - hi_top as f32, (y + h) * sy - hi_top as f32, hi_h)
        else {
            continue;
        };
        let src = [
            (colour[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (colour[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (colour[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            (colour[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        ];
        for py in y0..=y1 {
            let row = (py * hi_w) as usize;
            for px in x0..=x1 {
                let i = (row + px as usize) * 4;
                blend_rgba(&mut hi[i..i + 4], src);
            }
        }
    }

    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as u32;
    for y in 0..h {
        for x in 0..w {
            let mut alpha_sum = 0u32;
            let mut premul = [0u32; 3];
            for dy in 0..SUPERSAMPLE {
                for dx in 0..SUPERSAMPLE {
                    let src_i =
                        ((((y * SUPERSAMPLE + dy) * hi_w) + x * SUPERSAMPLE + dx) * 4) as usize;
                    let a = u32::from(hi[src_i + 3]);
                    alpha_sum += a;
                    for c in 0..3 {
                        premul[c] += u32::from(hi[src_i + c]) * a;
                    }
                }
            }
            let dst_i = ((y * w + x) * 4) as usize;
            rgba[dst_i + 3] = ((alpha_sum + samples / 2) / samples) as u8;
            if alpha_sum > 0 {
                for c in 0..3 {
                    rgba[dst_i + c] = ((premul[c] + alpha_sum / 2) / alpha_sum) as u8;
                }
            }
        }
    }
    Face { rgba, w, h }
}

fn span(a: f32, b: f32, limit: u32) -> Option<(u32, u32)> {
    let first = (a - 0.5).ceil().max(0.0) as u32;
    let end = ((b - 0.5).ceil().max(0.0) as u32).min(limit);
    (first < end).then(|| (first, end - 1))
}

fn blend_rgba(dst: &mut [u8], src: [u8; 4]) {
    let sa = u32::from(src[3]);
    if sa == 0 {
        return;
    }
    if sa == 255 {
        dst.copy_from_slice(&src);
        return;
    }
    let da = u32::from(dst[3]);
    let inv = 255 - sa;
    let out_a = sa * 255 + da * inv;
    if out_a == 0 {
        return;
    }
    for c in 0..3 {
        let value = u32::from(src[c]) * sa * 255 + u32::from(dst[c]) * da * inv;
        dst[c] = ((value + out_a / 2) / out_a) as u8;
    }
    dst[3] = ((out_a + 127) / 255) as u8;
}

pub fn draw_empty_slot(canvas: Canvas, colours: &Colours, out: &mut Vec<Draw>) {
    let l = Layout::of(canvas);
    draw_back(&l, colours, 1.0, out);
    draw_front(&l, colours, 1.0, out);
}

/// Centre of the slit, for printing the system name in it.
pub fn slit_centre(canvas: Canvas) -> (f32, f32) {
    let l = Layout::of(canvas);
    (l.cx, (l.slit_y + l.scoop_y + l.scoop_d) / 2.0)
}

fn band(x: f32, y: f32, w: f32, h: f32, c: [f32; 4], alpha: f32) -> Draw {
    Draw::Rect {
        x,
        y,
        w,
        h,
        colour: [c[0], c[1], c[2], c[3] * alpha],
    }
}

fn draw_back(l: &Layout, c: &Colours, alpha: f32, out: &mut Vec<Draw>) {
    out.push(band(l.bay_x, l.band_y, BAY_W, LIP_H, c.housing, alpha));
    out.push(band(l.mouth_x, l.band_y, MOUTH_W, LIP_H, c.edge, alpha));
    out.push(band(l.bay_x, l.bay_y, BAY_W, RECESS_H, c.recess, alpha));
    out.push(band(l.mouth_x, l.slit_y, MOUTH_W, SLIT_H, c.opening, alpha));
    for_each_scoop_span(l, |x, w, depth| {
        out.push(band(x, l.scoop_y, w, depth, c.opening, alpha));
    });
}

fn draw_front(l: &Layout, c: &Colours, alpha: f32, out: &mut Vec<Draw>) {
    front_bands(l, c, |x, y, w, h, col| {
        out.push(band(x, y, w, h, col, alpha))
    });
}

fn front_bands(l: &Layout, c: &Colours, mut band: impl FnMut(f32, f32, f32, f32, [f32; 4])) {
    let right = l.bay_x + BAY_W;
    let floor = l.scoop_y + l.scoop_d + RIM_W;
    band(0.0, l.band_y, l.bay_x, MOUTH_H, c.housing);
    band(right, l.band_y, l.w - right, MOUTH_H, c.housing);

    let near = l.cx - SCOOP_W / 2.0;
    band(
        l.bay_x,
        l.scoop_y,
        near - l.bay_x,
        floor - l.scoop_y,
        c.housing,
    );
    let far = l.cx + SCOOP_W / 2.0;
    band(far, l.scoop_y, right - far, floor - l.scoop_y, c.housing);

    for_each_scoop_span(l, |x, w, depth| {
        let top = l.scoop_y + depth + RIM_W;
        band(x, top, w, floor - top, c.housing);
    });
    band(0.0, floor, l.w, l.h - floor, c.housing);
    for_each_scoop_span(l, |x, w, depth| {
        band(x, l.scoop_y + depth, w, RIM_W, c.edge);
    });
}

fn for_each_scoop_span(l: &Layout, mut span: impl FnMut(f32, f32, f32)) {
    let hw = SCOOP_W / 2.0;
    let depth = |x: f32| l.scoop_d * (1.0 - (x.abs() / hw).powf(SCOOP_FLAT)).max(0.0);
    let mut x = -hw;
    while x < hw {
        let d = depth(x + 0.5).round();
        let start = x;
        while x < hw && depth(x + 0.5).round() == d {
            x += 1.0;
        }
        span(l.cx + start, x - start, d);
    }
}

fn travel(seat: f32, catch: f32) -> f32 {
    if seat < CATCH_IN {
        catch * ease(seat / CATCH_IN)
    } else if seat < CATCH_OUT {
        catch + CREEP * (seat - CATCH_IN) / (CATCH_OUT - CATCH_IN)
    } else {
        let caught = catch + CREEP;
        caught + (1.0 - caught) * ease((seat - CATCH_OUT) / (1.0 - CATCH_OUT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_store::Platform;

    #[test]
    fn mouth_fits_slot_seated_carts() {
        for p in Platform::ALL {
            assert_eq!(slot_ui::seated_box(p).0 as f32, SEATED_W);
        }
    }
}
