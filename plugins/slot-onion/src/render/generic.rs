use slot_ui::text::{draw_centred, fit_box, label_font};
use slot_ui::{clean_label, cover, label_colour};

use super::CartRenderer;
use crate::card::{Card, Game};
use crate::draw::Face;

const W: u32 = 300;
const H: u32 = 240;
const SEATED: (u32, u32) = (240, 192);
const RADIUS: f32 = 16.0;
const RIDGE_H: u32 = 30;
const LABEL: (u32, u32, u32, u32) = (22, 42, W - 44, H - 42 - 20);
const RECESS: u32 = 3;

/// A plain cart for any system without its own renderer: a shell tinted per
/// system, with the Onion box art (or the game name) as its label.
pub struct GenericCart;

impl CartRenderer for GenericCart {
    fn size(&self, _: &Game) -> (u32, u32) {
        (W, H)
    }

    fn seated(&self, _: &Game) -> (u32, u32) {
        SEATED
    }

    fn face(&self, card: &Card, game: &Game) -> Face {
        let system = system_of(game);
        let shell = game
            .shell
            .map(|choice| choice.colour)
            .unwrap_or_else(|| shell_colour(system));
        let mut rgba = vec![0u8; (W * H * 4) as usize];

        for y in 0..H {
            let shade = 1.08 - 0.18 * y as f32 / H as f32;
            let ridge = if y < RIDGE_H { 1.1 } else { 1.0 };
            for x in 0..W {
                let a = corner_alpha(x, y);
                if a == 0 {
                    continue;
                }
                let grip = y > 8 && y < RIDGE_H - 6 && x % 10 < 2 && (40..W - 40).contains(&x);
                let k = shade * ridge * if grip { 0.8 } else { 1.0 };
                put(&mut rgba, x, y, scale(shell, k), a);
            }
        }

        let (lx, ly, lw, lh) = LABEL;
        let recess = scale(shell, 0.55);
        for y in ly - RECESS..ly + lh + RECESS {
            for x in lx - RECESS..lx + lw + RECESS {
                put(&mut rgba, x, y, recess, 255);
            }
        }

        let art = game
            .label
            .as_deref()
            .and_then(|p| cover(&card.host(p), lw, lh))
            .unwrap_or_else(|| generated_label(&game.stem, lw, lh));
        for y in 0..lh {
            for x in 0..lw {
                let i = ((y * lw + x) * 4) as usize;
                put(
                    &mut rgba,
                    lx + x,
                    ly + y,
                    [art[i], art[i + 1], art[i + 2]],
                    255,
                );
            }
        }

        Face { rgba, w: W, h: H }
    }

    fn shadow(&self, _: &Card, _: &Game) -> Option<(String, Face)> {
        let mut rgba = Vec::with_capacity((W * H * 4) as usize);
        for y in 0..H {
            for x in 0..W {
                rgba.extend_from_slice(&[0, 0, 0, corner_alpha(x, y)]);
            }
        }
        Some(("generic".into(), Face { rgba, w: W, h: H }))
    }
}

/// `Emu/<SYS>/../../Roms/...` -> `SYS`
fn system_of(game: &Game) -> &str {
    game.rom.split('/').nth(1).unwrap_or("")
}

fn shell_colour(system: &str) -> [u8; 3] {
    let tint = label_colour(system);
    let grey = 70.0;
    tint.map(|c| (grey + (c as f32 - grey) * 0.35) as u8)
}

fn generated_label(stem: &str, w: u32, h: u32) -> Vec<u8> {
    let bg = label_colour(stem);
    let mut rgba: Vec<u8> = (0..w * h)
        .flat_map(|_| [bg[0], bg[1], bg[2], 255])
        .collect();
    if let Some(font) = label_font() {
        let pad = 14.0;
        let layout = fit_box(
            font,
            &clean_label(stem),
            w as f32 - 2.0 * pad,
            h as f32 - 2.0 * pad,
            4,
            34.0,
            12.0,
        );
        draw_centred(&mut rgba, w, h, &layout, [255, 255, 255]);
    }
    rgba
}

fn corner_alpha(x: u32, y: u32) -> u8 {
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let cx = fx.clamp(RADIUS, W as f32 - RADIUS);
    let cy = fy.clamp(RADIUS, H as f32 - RADIUS);
    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
    ((RADIUS + 0.5 - d).clamp(0.0, 1.0) * 255.0) as u8
}

fn scale(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * k).clamp(0.0, 255.0) as u8)
}

fn put(rgba: &mut [u8], x: u32, y: u32, c: [u8; 3], a: u8) {
    let i = ((y * W + x) * 4) as usize;
    rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
}
