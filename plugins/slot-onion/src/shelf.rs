//! slot's spring-scrolled cart carousel (slot-ui/src/shelf.rs), made generic
//! over cart size so any `CartRenderer` can sit on it.

use slot_ui::CART_H;

use crate::chrome::MOUTH_H;
use crate::draw::{Canvas, Draw, TexId};
use crate::input::Millis;

const SIDE_SCALE: f32 = 0.78;
const SIDE_ALPHA: f32 = 0.55;
const OMEGA: f32 = 16.0;
const PART: f32 = 130.0;
const SLOTS: i32 = 3;
const REPEAT_DELAY_MS: Millis = 400;
const REPEAT_MS: [Millis; 4] = [110, 85, 65, 50];

pub struct Item {
    pub size: (f32, f32),
    pub seated: (f32, f32),
    pub initial: char,
    pub face: Option<TexId>,
    pub shadow: Option<TexId>,
    /// Stand-in colour while the face is still being built.
    pub colour: [u8; 3],
}

pub struct Shelf {
    pub items: Vec<Item>,
    pub index: usize,
    pub scroll: f32,
    ride: f32,
    vel: f32,
    held: Option<(i32, Millis, usize)>,
}

/// Where a cart of height `h` rests: tall carts centre above the slot mouth,
/// short ones share a common foot line.
pub fn rest_y(canvas: Canvas, h: f32) -> f32 {
    if h > CART_H as f32 {
        (canvas.h - MOUTH_H - h) / 2.0
    } else {
        (canvas.h + CART_H as f32) / 2.0 - h
    }
}

pub fn foot_y(canvas: Canvas, h: f32) -> f32 {
    rest_y(canvas, h) + h
}

impl Shelf {
    pub fn new(items: Vec<Item>) -> Self {
        Shelf {
            items,
            index: 0,
            scroll: 0.0,
            ride: 0.0,
            vel: 0.0,
            held: None,
        }
    }

    pub fn select(&mut self, i: usize) {
        self.index = i;
        self.scroll = i as f32;
        self.ride = i as f32;
        self.vel = 0.0;
    }

    pub fn left(&mut self) {
        self.step(-1);
    }

    pub fn right(&mut self) {
        self.step(1);
    }

    pub fn hold(&mut self, by: i32, now: Millis) {
        self.step(by);
        self.held = Some((by, now + REPEAT_DELAY_MS, 0));
    }

    pub fn release_hold(&mut self) {
        self.held = None;
    }

    /// Jump to the next (+1) or previous (-1) initial, like slot's shelf.
    pub fn jump_letter(&mut self, by: i32) {
        let n = self.items.len();
        if n < 2 {
            return;
        }
        let wrap = |i: i32| i.rem_euclid(n as i32) as usize;
        let here = self.items[self.index].initial;
        if self.items.iter().all(|item| item.initial == here) {
            return;
        }

        let target = if by > 0 {
            let mut at = self.index;
            for _ in 0..n {
                at = wrap(at as i32 + 1);
                if self.items[at].initial != here {
                    break;
                }
            }
            at
        } else {
            let mut start = self.index;
            for _ in 0..n {
                let before = wrap(start as i32 - 1);
                if self.items[before].initial != here {
                    break;
                }
                start = before;
            }
            if start == self.index {
                let mut previous = wrap(start as i32 - 1);
                let letter = self.items[previous].initial;
                for _ in 0..n {
                    let before = wrap(previous as i32 - 1);
                    if self.items[before].initial != letter {
                        break;
                    }
                    previous = before;
                }
                previous
            } else {
                start
            }
        };

        let ahead = (target as i32 - self.index as i32).rem_euclid(n as i32);
        let delta = if by > 0 { ahead } else { ahead - n as i32 };
        self.index = target;
        self.ride += delta as f32;
        self.release_hold();
    }

    pub fn tick(&mut self, now: Millis) {
        let Some((by, due, fired)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        let rate = REPEAT_MS[fired.min(REPEAT_MS.len() - 1)];
        self.held = Some((by, now + rate, fired + 1));
    }

    fn step(&mut self, by: i32) {
        let n = self.items.len();
        if n < 2 {
            return;
        }
        self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
        self.ride += by as f32;
    }

    pub fn update(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        // Closed-form critically damped spring integration. Explicit Euler
        // changes the spring's speed and damping with the frame rate, which
        // made the carousel tween differently on desktop and the slower Mini.
        let error = self.scroll - self.scroll_target();
        let tangent = self.vel + OMEGA * error;
        let decay = (-OMEGA * dt).exp();
        self.scroll += (error + tangent * dt) * decay - error;
        self.vel = (self.vel - OMEGA * tangent * dt) * decay;
    }

    fn scroll_target(&self) -> f32 {
        let n = self.items.len();
        if n == 0 {
            return 0.0;
        }
        let n = n as f32;
        let from = self.ride;
        from + (self.index as f32 - from + n / 2.0).rem_euclid(n) - n / 2.0
    }

    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.items.len() as i32;
        match n {
            0 => None,
            1 => (off == 0).then_some(self.index),
            _ => Some((self.index as i32 + off).rem_euclid(n) as usize),
        }
    }

    pub fn on_screen(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for step in 0..=SLOTS {
            for off in [step, -step] {
                if let Some(i) = self.cart_at_offset(off) {
                    if !out.contains(&i) {
                        out.push(i);
                    }
                }
            }
        }
        out
    }

    /// Left edge and scale of the selected cart, where an insert starts from.
    pub fn selected_at(&self, canvas: Canvas) -> (f32, f32) {
        let w = self.items.get(self.index).map_or(0.0, |c| c.size.0);
        let offset = self.scroll_target() - self.scroll;
        let scale = shrink(offset);
        (canvas.w / 2.0 + offset * w - w * scale / 2.0, scale)
    }

    pub fn draw_row(
        &self,
        canvas: Canvas,
        hidden: Option<usize>,
        recede: f32,
        out: &mut Vec<Draw>,
    ) {
        let recede = recede.clamp(0.0, 1.0);
        let target = self.scroll_target();
        for slot in -SLOTS..=SLOTS {
            let Some(i) = self.cart_at_offset(slot) else {
                continue;
            };
            if hidden == Some(i) {
                continue;
            }
            let item = &self.items[i];
            let offset = target + slot as f32 - self.scroll;
            let t = offset.abs().min(1.0);
            let scale = shrink(offset);
            let alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
            let (cw, ch) = item.size;
            let (w, h) = (cw * scale, ch * scale);
            let away = offset.signum() * (1.0 + offset.abs());
            let x = canvas.w / 2.0 + offset * cw - w / 2.0 + away * PART * recede;
            if x + w <= 0.0 || x >= canvas.w || alpha <= 0.0 {
                continue;
            }
            let y = foot_y(canvas, ch) - h;
            if alpha < 1.0 {
                if let Some(tex) = item.shadow {
                    out.push(Draw::Tex {
                        x,
                        y,
                        w,
                        h,
                        tex,
                        alpha: (alpha / SIDE_ALPHA).clamp(0.0, 1.0),
                    });
                }
            }
            out.push(match item.face {
                Some(tex) => Draw::Tex {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                },
                None => Draw::Rect {
                    x,
                    y,
                    w,
                    h,
                    colour: {
                        let [r, g, b, _] = crate::draw::rgb(item.colour);
                        [r, g, b, alpha]
                    },
                },
            });
        }
    }
}

fn shrink(offset: f32) -> f32 {
    1.0 + (SIDE_SCALE - 1.0) * offset.abs().min(1.0)
}
