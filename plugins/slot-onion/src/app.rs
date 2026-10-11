//! The frontend's phases, cut down from slot's `App` (crates/slot/src/app.rs):
//! browse the shelf, insert a cart (then hand the launch to Onion and exit),
//! or start by ejecting the game that just closed.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Instant;

use slot_gfx::{screen_brightness, screen_scale, screen_width};
use slot_ui::{cover, label_colour};

use crate::card::{normalize, Card, System};
use crate::chrome::{draw_empty_slot, slit_centre, ChromeTextures, Colours, SlotChrome};
use crate::draw::{rgb, Canvas, Draw, Face, TexId, TextureKind, Textures};
use crate::input::{Btn, Event, Millis};
use crate::render::Registry;
use crate::shelf::{Item, Shelf};
use crate::text::text_face;

// slot's timings.
const INSERT_S: f32 = 0.73;
const INSERT_HOLD_S: f32 = 0.28;
const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;
const POST_INSERT_LAUNCH_WAIT_S: f32 = 1.5;
const EJECT_S: f32 = SEATED_AT;
const EJECT_HOLD_S: f32 = 0.35;
const POWER_OFF_S: f32 = 0.16;
const INSERT_SOUND_LEAD_S: f32 = 0.097;
const CLEAN_LAUNCH_HOLD_MS: Millis = 500;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SoundCue {
    Insert,
    Eject,
}

const SELECTION: &str = ".tmp_update/config/frontend/selection";
const MAINUI_RECENTS_STATE: &str = r#"{"list":[{"title":157,"type":0,"currpos":0,"pagestart":0,"pageend":3},{"title":18,"type":10,"currpos":0,"pagestart":0,"pageend":5}]}"#;
const TITLE_Y: f32 = 34.0;

pub enum Exit {
    /// Launch this game: (system, game) indices.
    Launch(usize, usize, bool),
    /// Open Onion's stock MainUI once (Settings, Apps, ...).
    StockMainUi,
}

enum Phase {
    Shelf,
    Inserting { t: f32, clean: bool },
    Ejecting { t: f32 },
    Done(Exit),
}

struct Label {
    tex: TexId,
    w: f32,
    h: f32,
}

pub struct App {
    canvas: Canvas,
    panel: (u32, u32),
    card: Card,
    systems: Vec<System>,
    registry: Registry,
    shelves: Vec<Shelf>,
    at: usize,
    phase: Phase,
    screen: f32,
    shot: Option<TexId>,
    colours: Colours,
    chrome: Option<ChromeTextures>,
    shadows: HashMap<String, TexId>,
    names: Vec<Option<Label>>,
    title: Option<((usize, usize), Option<Label>)>,
    sfx: Option<SoundCue>,
    battery: Option<u8>,
    battery_label: Option<(u8, Option<Label>)>,
    time: String,
    time_label: Option<(String, Option<Label>)>,
    launch_held: Option<Millis>,
    sync_profile_us: [u64; 4],
}

impl App {
    pub fn new(card: Card, canvas: Canvas, panel: (u32, u32)) -> Self {
        let systems = order(card.systems());
        let registry = Registry::new();
        let shelves = systems
            .iter()
            .map(|s| {
                let renderer = registry.get(&s.id);
                Shelf::new(
                    s.games
                        .iter()
                        .map(|g| {
                            let (w, h) = renderer.size(g);
                            let (sw, sh) = renderer.seated(g);
                            Item {
                                size: (w as f32, h as f32),
                                seated: (sw as f32, sh as f32),
                                initial: slot_store::initial(&g.stem),
                                face: None,
                                shadow: None,
                                colour: label_colour(&g.stem),
                            }
                        })
                        .collect(),
                )
            })
            .collect();
        let names = systems.iter().map(|_| None).collect();
        let mut app = App {
            canvas,
            panel,
            card,
            systems,
            registry,
            shelves,
            at: 0,
            phase: Phase::Shelf,
            screen: 0.0,
            shot: None,
            colours: Colours::default(),
            chrome: None,
            shadows: HashMap::new(),
            names,
            title: None,
            sfx: None,
            battery: None,
            battery_label: None,
            time: String::new(),
            time_label: None,
            launch_held: None,
            sync_profile_us: [0; 4],
        };
        app.restore_selection();
        app
    }

    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Update the status values shown in the top corners of the shelf.
    pub fn set_status(&mut self, battery: Option<u8>, time: String) {
        self.battery = battery.map(|p| p.min(100));
        self.time = time;
    }

    /// Start by ejecting the game that just closed, if it's on the shelf.
    /// `last_game` is runtime.sh's realpath'd rompath.
    pub fn eject(&mut self, last_game: &str, gfx: &mut dyn Textures) {
        let want = normalize(last_game);
        let found = self.systems.iter().enumerate().find_map(|(s, sys)| {
            sys.games
                .iter()
                .position(|g| normalize(&Card::device(&g.rom)) == want)
                .map(|g| (s, g))
        });
        let Some((s, g)) = found else {
            return;
        };
        self.at = s;
        self.shelves[s].select(g);

        // keymon names the screenshot after the top recents entry, spelled as
        // whoever launched the game wrote it.
        let rompath = self
            .card
            .top_recent()
            .filter(|r| normalize(r) == want)
            .unwrap_or_else(|| Card::device(&self.systems[s].games[g].rom));
        let (w, h) = self.panel;
        self.shot = cover(&self.card.romscreen(&rompath), w, h)
            .map(|rgba| gfx.create_texture(&Face { rgba, w, h }, TextureKind::Other));
        self.screen = if self.shot.is_some() { 1.0 } else { 0.0 };
        self.phase = Phase::Ejecting { t: -EJECT_HOLD_S };
    }

    pub fn event(&mut self, event: Event, now: Millis) {
        if let Event::Release(Btn::A) = event {
            self.launch_held = None;
        }
        let Phase::Shelf = self.phase else {
            return;
        };
        let shelf = &mut self.shelves[self.at];
        match event {
            Event::Press(Btn::Up) => shelf.jump_letter(-1),
            Event::Press(Btn::Down) => shelf.jump_letter(1),
            Event::Press(Btn::Left) => shelf.hold(-1, now),
            Event::Press(Btn::Right) => shelf.hold(1, now),
            Event::Release(Btn::Left | Btn::Right) => shelf.release_hold(),
            Event::Press(Btn::L1) => self.switch(-1),
            Event::Press(Btn::R1) => self.switch(1),
            Event::Press(Btn::A) if !shelf.items.is_empty() => {
                self.launch_held = Some(now);
                self.phase = Phase::Inserting {
                    t: 0.0,
                    clean: false,
                };
            }
            Event::Press(Btn::Select | Btn::Y) => self.phase = Phase::Done(Exit::StockMainUi),
            _ => {}
        }
    }

    fn switch(&mut self, by: i32) {
        let n = self.shelves.len() as i32;
        self.shelves[self.at].release_hold();
        self.at = (self.at as i32 + by).rem_euclid(n) as usize;
    }

    pub fn update(&mut self, dt: f32, now: Millis) {
        if self
            .launch_held
            .is_some_and(|pressed| now.saturating_sub(pressed) >= CLEAN_LAUNCH_HOLD_MS)
        {
            self.launch_held = None;
            if let Phase::Inserting { clean, .. } = &mut self.phase {
                *clean = true;
            }
        }
        let shelf = &mut self.shelves[self.at];
        let mut sound = None;
        let next = match &mut self.phase {
            Phase::Shelf => {
                shelf.tick(now);
                // The spring integrator is time-based and stable across the
                // slower, variable frame intervals of the software renderer.
                shelf.update(dt);
                None
            }
            Phase::Inserting { t, clean } => {
                let was = *t;
                *t += dt;
                if was < SEATED_AT - INSERT_SOUND_LEAD_S && *t >= SEATED_AT - INSERT_SOUND_LEAD_S {
                    sound = Some(SoundCue::Insert);
                }
                (*t >= SEATED_AT + POST_INSERT_LAUNCH_WAIT_S).then_some(Phase::Done(Exit::Launch(
                    self.at,
                    shelf.index,
                    *clean,
                )))
            }
            Phase::Ejecting { t } => {
                if self.screen <= 0.0 {
                    let was = *t;
                    *t += dt;
                    if was < 0.0 && *t >= 0.0 {
                        sound = Some(SoundCue::Eject);
                    }
                }
                (*t >= EJECT_S).then_some(Phase::Shelf)
            }
            Phase::Done(_) => None,
        };
        if let Some(phase) = next {
            self.phase = phase;
        }
        if sound.is_some() {
            self.sfx = sound;
        }
        self.screen = (self.screen - dt / POWER_OFF_S).max(0.0);
    }

    /// Take a cartridge sound cue for the host playback backend.
    pub fn take_sfx(&mut self) -> Option<SoundCue> {
        self.sfx.take()
    }

    fn seat(&self) -> f32 {
        match self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { t, .. } => (t / SEATED_AT).clamp(0.0, 1.0),
            Phase::Ejecting { t } => 1.0 - (t / EJECT_S).clamp(0.0, 1.0),
            Phase::Done(_) => 1.0,
        }
    }

    pub fn exit(&self) -> Option<&Exit> {
        match &self.phase {
            Phase::Done(exit) => Some(exit),
            _ => None,
        }
    }

    /// Hand off to runtime.sh: queue the launch (and record it in recents the
    /// way MainUI would), or ask for stock MainUI.
    pub fn finish(&self, tmp: &Path) -> io::Result<()> {
        let Some(exit) = self.exit() else {
            return Ok(());
        };
        self.save_selection();
        match *exit {
            Exit::Launch(s, g, clean) => {
                let (system, game) = (&self.systems[s], &self.systems[s].games[g]);
                self.card.push_recent(system, game)?;
                let reset = tmp.join("reset_game");
                if clean {
                    let supports_reset = fs::read_to_string(self.card.host(&system.launch))
                        .is_ok_and(|script| script.contains(".retroarch/cores"));
                    if supports_reset {
                        fs::write(&reset, "")?;
                    } else {
                        let _ = fs::remove_file(&reset);
                        eprintln!("slot-onion: clean launch is unsupported by {}", system.id);
                    }
                } else {
                    let _ = fs::remove_file(&reset);
                }
                fs::write(
                    tmp.join("cmd_to_run.sh"),
                    Card::launch_cmd(system, game) + "\n",
                )
            }
            Exit::StockMainUi => {
                let system = &self.systems[self.at];
                let game = &system.games[self.shelves[self.at].index];
                // Put the selected cart first in Recents and ask MainUI to
                // restore that tab/row when the runtime hands off to it.
                self.card.push_recent(system, game)?;
                fs::write(tmp.join("state.json"), MAINUI_RECENTS_STATE)?;
                fs::write(tmp.join("use_stock_mainui"), "")
            }
        }
    }

    /// Build cart faces for whatever is on screen, at most `budget` per call so
    /// a slow device keeps animating while the shelf fills in.
    pub fn sync(&mut self, gfx: &mut dyn Textures, mut budget: usize) {
        self.sync_profile_us = [0; 4];
        if self.chrome.is_none() {
            let started = Instant::now();
            self.chrome = Some(crate::chrome::prerender(
                self.canvas,
                self.panel,
                &self.colours,
                gfx,
            ));
            self.sync_profile_us[0] = started.elapsed().as_micros() as u64;
        }
        let canvas = self.canvas;
        let system = &self.systems[self.at];
        let renderer = self.registry.get(&system.id);
        let shelf = &mut self.shelves[self.at];
        for i in shelf.on_screen() {
            if budget == 0 {
                break;
            }
            let item = &mut shelf.items[i];
            if item.face.is_some() {
                continue;
            }
            let game = &system.games[i];
            let started = Instant::now();
            item.face =
                Some(gfx.create_texture(&renderer.face(&self.card, game), TextureKind::Cart));
            self.sync_profile_us[1] =
                self.sync_profile_us[1].saturating_add(started.elapsed().as_micros() as u64);
            let started = Instant::now();
            if let Some((key, face)) = renderer.shadow(&self.card, game) {
                let tex = *self
                    .shadows
                    .entry(key)
                    .or_insert_with(|| gfx.create_texture(&face, TextureKind::Shadow));
                item.shadow = Some(tex);
            }
            self.sync_profile_us[2] =
                self.sync_profile_us[2].saturating_add(started.elapsed().as_micros() as u64);
            budget -= 1;
        }

        let labels_started = Instant::now();
        if self.names[self.at].is_none() {
            self.names[self.at] = label(gfx, &system.label, 13.0, [0x9a, 0x9a, 0xa4], 220.0);
        }
        if self.battery_label.as_ref().map(|b| b.0) != self.battery {
            self.battery_label = self.battery.map(|p| {
                (
                    p,
                    label(gfx, &format!("{p}%"), 13.0, [0xe8, 0xe8, 0xee], 46.0),
                )
            });
        }
        if self.time_label.as_ref().map(|t| t.0.as_str()) != Some(self.time.as_str()) {
            let face = if self.time.is_empty() {
                None
            } else {
                label(gfx, &self.time, 18.0, [0xe8, 0xe8, 0xee], 90.0)
            };
            self.time_label = Some((self.time.clone(), face));
        }
        let key = (self.at, shelf.index);
        if self.title.as_ref().map(|t| t.0) != Some(key) {
            let stem = system
                .games
                .get(shelf.index)
                .map_or("", |g| g.stem.as_str());
            let face = label(
                gfx,
                &slot_ui::clean_label(stem),
                22.0,
                [0xe8, 0xe8, 0xee],
                canvas.w - 260.0,
            );
            self.title = Some((key, face));
        }
        self.sync_profile_us[3] = labels_started.elapsed().as_micros() as u64;
    }

    pub fn take_sync_profile(&mut self) -> [u64; 4] {
        std::mem::take(&mut self.sync_profile_us)
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        let canvas = self.canvas;
        let shelf = &self.shelves[self.at];
        match self.phase {
            Phase::Shelf => {
                shelf.draw_row(canvas, None, 0.0, out);
                if let Some(chrome) = &self.chrome {
                    chrome.draw_empty(out);
                } else {
                    draw_empty_slot(canvas, &self.colours, out);
                }
                if let Some((_, Some(title))) = &self.title {
                    push_label(title, canvas.w / 2.0, TITLE_Y, 1.0, out);
                }
                self.draw_status(out);
                if let Some(name) = &self.names[self.at] {
                    let (x, y) = slit_centre(canvas);
                    push_label(name, x, y, 1.0, out);
                }
            }
            _ => {
                let seat = self.seat();
                shelf.draw_row(canvas, Some(shelf.index), seat, out);
                if let Some(item) = shelf.items.get(shelf.index) {
                    let (rest, scale) = shelf.selected_at(canvas);
                    SlotChrome {
                        colours: &self.colours,
                        cached: self.chrome.as_ref(),
                        size: item.size,
                        seated: item.seated,
                        face: item.face,
                        colour: rgb(item.colour),
                        rest,
                        scale,
                        seat,
                        screen: self.screen,
                    }
                    .draw(canvas, out);
                }
                self.draw_shot(out);
            }
        }
    }

    fn draw_status(&self, out: &mut Vec<Draw>) {
        // Match slot-ui's footer: centred in the cartridge-mouth band.
        const MOUTH_H: f32 = 58.0;
        const HINT_H: f32 = 24.0;
        const GAUGE_H: f32 = 11.0;
        let footer_y = self.canvas.h - MOUTH_H + (MOUTH_H - HINT_H) / 2.0;
        if let Some((percent, label)) = &self.battery_label {
            let (x, top, w, h, wall) = (24.0, footer_y + (HINT_H - GAUGE_H) / 2.0, 22.0, 12.0, 1.5);
            let ink = [232.0 / 255.0, 232.0 / 255.0, 238.0 / 255.0, 1.0];
            let rect = |x, y, w, h| Draw::Rect {
                x,
                y,
                w,
                h,
                colour: ink,
            };
            out.push(rect(x, top, w, wall));
            out.push(rect(x, top + h - wall, w, wall));
            out.push(rect(x, top, wall, h));
            out.push(rect(x + w - wall, top, wall, h));
            out.push(rect(x + w, top + 4.0, 3.0, 4.0));
            let inner = (w - 2.0 * wall - 2.0).max(0.0) * f32::from(*percent) / 100.0;
            if inner > 0.0 {
                out.push(rect(x + wall + 1.0, top + wall, inner, h - 2.0 * wall));
            }
            if let Some(label) = label {
                push_label(
                    label,
                    72.0 + label.w / 2.0,
                    footer_y + HINT_H / 2.0,
                    1.0,
                    out,
                );
            }
        }
        if let Some((_, Some(time))) = &self.time_label {
            push_label(
                time,
                self.canvas.w - 24.0 - time.w / 2.0,
                footer_y + HINT_H / 2.0,
                1.0,
                out,
            );
        }
    }

    /// The closing game's last frame, collapsing like slot's CRT power-off.
    fn draw_shot(&self, out: &mut Vec<Draw>) {
        let (Some(tex), true) = (self.shot, self.screen > 0.0) else {
            return;
        };
        let t = self.screen;
        let (cw, ch) = (self.canvas.w, self.canvas.h);
        let (w, h) = (cw * screen_width(t), ch * screen_scale(t));
        let (x, y) = ((cw - w) / 2.0, (ch - h) / 2.0);
        out.push(Draw::Tex {
            x,
            y,
            w,
            h,
            tex,
            alpha: 1.0,
        });
        let glow = (screen_brightness(t) - 1.0) * 0.6;
        if glow > 0.0 {
            out.push(Draw::Rect {
                x,
                y,
                w,
                h,
                colour: [1.0, 1.0, 1.0, glow],
            });
        }
    }

    fn restore_selection(&mut self) {
        let Ok(text) = fs::read_to_string(self.card.host(SELECTION)) else {
            return;
        };
        let mut lines = text.lines();
        let (Some(id), Some(stem)) = (lines.next(), lines.next()) else {
            return;
        };
        if let Some(s) = self.systems.iter().position(|s| s.id == id) {
            self.at = s;
            if let Some(g) = self.systems[s].games.iter().position(|g| g.stem == stem) {
                self.shelves[s].select(g);
            }
        }
    }

    fn save_selection(&self) {
        let system = &self.systems[self.at];
        let Some(game) = system.games.get(self.shelves[self.at].index) else {
            return;
        };
        let path = self.card.host(SELECTION);
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, format!("{}\n{}\n", system.id, game.stem));
    }
}

/// Game Boy family first, in slot's order, then everything else by name.
fn order(mut systems: Vec<System>) -> Vec<System> {
    const FIRST: [&str; 3] = ["GBA", "GBC", "GB"];
    systems.sort_by_key(|s| {
        (
            FIRST.iter().position(|f| *f == s.id).unwrap_or(FIRST.len()),
            s.label.to_lowercase(),
        )
    });
    systems
}

fn label(
    gfx: &mut dyn Textures,
    text: &str,
    px: f32,
    colour: [u8; 3],
    max_w: f32,
) -> Option<Label> {
    let face = text_face(text, px, colour, max_w)?;
    Some(Label {
        w: face.w as f32,
        h: face.h as f32,
        tex: gfx.create_texture(&face, TextureKind::Ui),
    })
}

fn push_label(l: &Label, cx: f32, cy: f32, alpha: f32, out: &mut Vec<Draw>) {
    out.push(Draw::Tex {
        x: (cx - l.w / 2.0).round(),
        y: (cy - l.h / 2.0).round(),
        w: l.w,
        h: l.h,
        tex: l.tex,
        alpha,
    });
}
