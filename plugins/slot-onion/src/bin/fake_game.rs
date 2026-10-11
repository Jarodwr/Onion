//! Stands in for RetroArch on the desktop: shows a test pattern for the ROM,
//! and on Menu/Esc saves a screenshot to romScreens the way keymon does
//! (named after the top recents rompath), then exits.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use slot_onion::card::Card;
use slot_onion::draw::{rgb, Canvas, Draw, TextureKind, Textures};
use slot_onion::host::{window, Args};
use slot_onion::soft::Soft;
use slot_onion::text::text_face;
use slot_ui::label_colour;

fn main() -> ExitCode {
    let args = match Args::parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("fake-game: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(rom) = args.rom.clone() else {
        eprintln!("fake-game: --rom <rompath> is required");
        return ExitCode::from(2);
    };
    let card = Card::new(&args.sdcard);
    let stem = Path::new(&rom)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("game")
        .to_owned();

    let canvas = Canvas::for_panel(args.panel.0, args.panel.1);
    let mut soft = Soft::new(args.panel.0, args.panel.1, canvas);
    let title = text_face(&stem, 30.0, [255, 255, 255], canvas.w - 60.0).map(|f| {
        (
            f.w as f32,
            f.h as f32,
            soft.create_texture(&f, TextureKind::Other),
        )
    });
    let hint = text_face("Esc to exit game", 16.0, [255, 255, 255], 400.0).map(|f| {
        (
            f.w as f32,
            f.h as f32,
            soft.create_texture(&f, TextureKind::Other),
        )
    });

    let mut window = match window(&format!("fake-game: {stem}"), &args) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("fake-game: window: {e}");
            return ExitCode::FAILURE;
        }
    };

    let bg = rgb(label_colour(&stem));
    let start = Instant::now();
    let mut draws = Vec::new();
    while window.is_open() && !window.is_key_down(minifb::Key::Escape) {
        let t = start.elapsed().as_secs_f32();
        draws.clear();
        draws.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: canvas.w,
            h: canvas.h,
            colour: bg,
        });
        // Colour bars and a bouncing block so the screenshot is recognisable.
        for i in 0..8 {
            let c = [
                (i & 1) as f32,
                ((i >> 1) & 1) as f32,
                ((i >> 2) & 1) as f32,
                1.0,
            ];
            let w = canvas.w / 8.0;
            draws.push(Draw::Rect {
                x: i as f32 * w,
                y: canvas.h - 90.0,
                w,
                h: 90.0,
                colour: c,
            });
        }
        let bx = (canvas.w - 60.0) * (0.5 + 0.5 * (t * 1.7).sin());
        let by = 140.0 + 120.0 * (0.5 + 0.5 * (t * 2.3).cos());
        draws.push(Draw::Rect {
            x: bx,
            y: by,
            w: 60.0,
            h: 60.0,
            colour: [1.0, 1.0, 1.0, 1.0],
        });
        for (face, y) in [(title, 60.0), (hint, canvas.h - 120.0)] {
            if let Some((w, h, tex)) = face {
                draws.push(Draw::Tex {
                    x: (canvas.w - w) / 2.0,
                    y,
                    w,
                    h,
                    tex,
                    alpha: 1.0,
                });
            }
        }
        soft.clear(0);
        soft.draw(&draws);
        if window
            .update_with_buffer(&soft.fb, soft.w as usize, soft.h as usize)
            .is_err()
        {
            break;
        }
    }

    // keymon's screenshot_system(): name it after the top recents entry.
    let rompath = card.top_recent().unwrap_or(rom);
    let out = card.romscreen(&rompath);
    match save_png(&out, &soft) {
        Ok(()) => eprintln!("fake-game: romScreen {}", out.display()),
        Err(e) => eprintln!("fake-game: romScreen: {e}"),
    }
    ExitCode::SUCCESS
}

fn save_png(path: &Path, soft: &Soft) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), soft.w, soft.h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = soft
        .fb
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    encoder.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
