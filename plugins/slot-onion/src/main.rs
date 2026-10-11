//! Desktop build of the frontend. Behaves like it will under runtime.sh:
//! reads `<tmp>/last_game`, and on exit leaves `<tmp>/cmd_to_run.sh` or
//! `<tmp>/use_stock_mainui` behind.

use std::fs;
use std::process::{Command, ExitCode};
use std::time::Instant;

use minifb::KeyRepeat;

use slot_onion::app::App;
use slot_onion::audio::Audio;
use slot_onion::card::Card;
use slot_onion::draw::Canvas;
use slot_onion::host::{button, window, Args, KEY_HELP};
use slot_onion::input::{Btn, Event};
use slot_onion::soft::Soft;

/// Faces built per frame; one cart face is a few ms on desktop.
const FACE_BUDGET: usize = 2;

fn main() -> ExitCode {
    let args = match Args::parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("slot-onion: {e}");
            return ExitCode::from(2);
        }
    };
    let canvas = Canvas::for_panel(args.panel.0, args.panel.1);
    let mut soft = Soft::new(args.panel.0, args.panel.1, canvas);
    let mut app = App::new(Card::new(&args.sdcard), canvas, args.panel);
    app.set_status(read_battery(&args.tmp), local_time());
    let audio = match Audio::open() {
        Ok(audio) => Some(audio),
        Err(e) => {
            eprintln!("slot-onion: audio unavailable: {e}");
            None
        }
    };

    if app.is_empty() {
        eprintln!("slot-onion: no games under {}", args.sdcard.display());
        let _ = fs::write(args.tmp.join("use_stock_mainui"), "");
        return ExitCode::SUCCESS;
    }
    if let Ok(last) = fs::read_to_string(args.tmp.join("last_game")) {
        app.eject(last.trim(), &mut soft);
    }

    let mut window = match window("slot-onion", &args) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("slot-onion: window: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("slot-onion: {KEY_HELP}\nslot-onion: Select opens stock MainUI, Esc quits");

    let start = Instant::now();
    let mut last = start;
    let mut last_status = start;
    let mut draws = Vec::new();
    while window.is_open() {
        let now_i = Instant::now();
        let dt = (now_i - last).as_secs_f32();
        last = now_i;
        let now = (now_i - start).as_millis() as u64;

        for key in window.get_keys_pressed(KeyRepeat::No) {
            match button(key) {
                // Menu on the desktop closes the window, like killing MainUI.
                Some(Btn::Menu) => return ExitCode::from(1),
                Some(b) => app.event(Event::Press(b), now),
                None => {}
            }
        }
        for key in window.get_keys_released() {
            if let Some(b) = button(key) {
                app.event(Event::Release(b), now);
            }
        }

        app.update(dt, now);
        if last_status.elapsed().as_secs() >= 5 {
            app.set_status(read_battery(&args.tmp), local_time());
            last_status = Instant::now();
        }
        if let (Some(audio), Some(sfx)) = (&audio, app.take_sfx()) {
            audio.play(sfx);
        }
        app.sync(&mut soft, FACE_BUDGET);
        draws.clear();
        app.draw(&mut draws);
        soft.clear(0);
        soft.draw(&draws);
        if let Err(e) = window.update_with_buffer(&soft.fb, soft.w as usize, soft.h as usize) {
            eprintln!("slot-onion: present: {e}");
            return ExitCode::FAILURE;
        }

        if app.exit().is_some() {
            if let Err(e) = app.finish(&args.tmp) {
                eprintln!("slot-onion: handoff: {e}");
                return ExitCode::FAILURE;
            }
            return ExitCode::SUCCESS;
        }
    }
    ExitCode::from(1)
}

fn read_battery(tmp: &std::path::Path) -> Option<u8> {
    fs::read_to_string(tmp.join("percBat"))
        .ok()?
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|percent| *percent <= 100)
}

fn local_time() -> String {
    Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}
