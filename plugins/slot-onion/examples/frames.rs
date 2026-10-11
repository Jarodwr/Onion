//! Renders key frames headlessly to PNGs, for eyeballing layout changes:
//!   cargo run --example frames -- <card> <out-dir> [WxH]
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

use slot_onion::app::App;
use slot_onion::card::{normalize, Card};
use slot_onion::draw::Canvas;
use slot_onion::input::{Btn, Event};
use slot_onion::soft::Soft;

const DT: f32 = 1.0 / 60.0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let card = &args[1];
    let out = Path::new(&args[2]);
    let (w, h) = args
        .get(3)
        .and_then(|s| s.split_once('x'))
        .map_or((640, 480), |(w, h)| {
            (w.parse().unwrap(), h.parse().unwrap())
        });
    fs::create_dir_all(out).unwrap();
    let canvas = Canvas::for_panel(w, h);

    // Shelf for each system, then an insert.
    let mut soft = Soft::new(w, h, canvas);
    let mut app = App::new(Card::new(card), canvas, (w, h));
    let now = std::cell::Cell::new(0u64);
    let step = |app: &mut App, soft: &mut Soft, frames: usize, name: Option<&str>| {
        for _ in 0..frames {
            now.set(now.get() + 16);
            app.update(DT, now.get());
            app.sync(soft, 16);
        }
        if let Some(name) = name {
            let mut draws = Vec::new();
            app.draw(&mut draws);
            soft.clear(0);
            soft.draw(&draws);
            save(&out.join(format!("{name}.png")), soft);
        }
    };
    for i in 0..5 {
        step(&mut app, &mut soft, 30, Some(&format!("shelf-{i}")));
        app.event(Event::Press(Btn::R1), now.get());
    }
    app.event(Event::Press(Btn::Right), now.get());
    app.event(Event::Release(Btn::Right), now.get());
    step(&mut app, &mut soft, 6, Some("scrolling"));
    step(&mut app, &mut soft, 30, None);
    app.event(Event::Press(Btn::A), now.get());
    for f in [8, 8, 8, 30] {
        step(
            &mut app,
            &mut soft,
            f,
            Some(&format!("insert-{}", now.get())),
        );
    }

    // Eject the first GBA game, with a fake romScreen.
    let c = Card::new(card);
    let gba = c.systems().into_iter().find(|s| s.id == "GBA").unwrap();
    let rom = Card::device(&gba.games[0].rom);
    let shot = c.romscreen(&rom);
    if !shot.exists() {
        let mut fake = Soft::new(w, h, canvas);
        fake.clear(0x3060c0);
        save(&shot, &fake);
    }
    let mut app = App::new(Card::new(card), canvas, (w, h));
    app.eject(&normalize(&rom), &mut soft);
    for f in [0, 4, 6, 20, 12, 12, 30] {
        step(
            &mut app,
            &mut soft,
            f,
            Some(&format!("eject-{}", now.get())),
        );
    }
}

fn save(path: &Path, soft: &Soft) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), soft.w, soft.h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = soft
        .fb
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}
