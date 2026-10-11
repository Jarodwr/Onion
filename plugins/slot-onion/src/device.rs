//! Miyoo Mini frontend, using Onion's Linux framebuffer and evdev devices.

use std::fs;
use std::io;
use std::os::fd::AsRawFd;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use slot_onion::app::App;
use slot_onion::args::Args;
use slot_onion::audio::Audio;
use slot_onion::card::Card;
use slot_onion::draw::Canvas;
use slot_onion::input::{Btn, Event};
use slot_onion::soft::Soft;

const FACE_BUDGET: usize = 1;
const FRAME_TIME: Duration = Duration::from_micros(16_667);
const PROFILE_FRAMES: u32 = 30;
const PROFILE_STAGES: [&str; 5] = ["input", "update", "sync", "raster", "present"];
const SYNC_PARTS: [&str; 4] = ["chrome-build", "cart-build", "shadow-build", "label-build"];
const DRAW_PARTS: [&str; 6] = [
    "rects",
    "cart-tex",
    "shadow-tex",
    "chrome-tex",
    "ui-tex",
    "other-tex",
];

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("slot-onion: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args = Args::parse().map_err(io::Error::other)?;
    eprintln!("slot-onion: opening framebuffer");
    let mut display = Framebuffer::open("/dev/fb0")?;
    eprintln!("slot-onion: framebuffer initialized");
    let panel = (display.width, display.height);
    let canvas = Canvas::for_panel(panel.0, panel.1);
    eprintln!("slot-onion: creating software canvas");
    let mut soft = Soft::new_threaded(panel.0, panel.1, canvas)?;
    eprintln!("slot-onion: loading cartridge catalog");
    let mut app = App::new(Card::new(&args.sdcard), canvas, panel);
    eprintln!("slot-onion: cartridge catalog loaded");
    app.set_status(read_battery(&args.tmp), local_time());

    if app.is_empty() {
        eprintln!("slot-onion: no games under {}", args.sdcard.display());
        fs::write(args.tmp.join("use_stock_mainui"), "")?;
        return Ok(ExitCode::SUCCESS);
    }

    if let Ok(last) = fs::read_to_string(args.tmp.join("last_game")) {
        app.eject(last.trim(), &mut soft);
    }

    let audio = match Audio::open() {
        Ok(audio) => Some(audio),
        Err(error) => {
            eprintln!("slot-onion: audio worker unavailable: {error}");
            None
        }
    };
    eprintln!("slot-onion: opening input");
    let mut input = Evdev::open("/dev/input/event0")?;
    eprintln!("slot-onion: device frontend ready ({panel:?})");

    let start = Instant::now();
    let mut last = start;
    let mut last_status = start;
    let mut first_frame = true;
    let mut draws = Vec::new();
    let mut profile = FrameProfile::default();
    loop {
        let frame_start = Instant::now();
        let frame_interval = frame_start - last;
        let dt = frame_interval.as_secs_f32();
        last = frame_start;
        profile.record_interval(frame_interval);
        let now = frame_start.duration_since(start).as_millis() as u64;

        let stage = Instant::now();
        for event in input.poll()? {
            if event == Event::Press(Btn::Menu) {
                // Onion treats an unexpected/non-zero frontend exit as a
                // request to return to its stock MainUI for this boot.
                return Ok(ExitCode::from(1));
            }
            app.event(event, now);
        }
        profile.record(0, stage.elapsed());

        let stage = Instant::now();
        app.update(dt, now);
        if last_status.elapsed() >= Duration::from_secs(5) {
            app.set_status(read_battery(&args.tmp), local_time());
            last_status = Instant::now();
        }
        if let (Some(audio), Some(sfx)) = (&audio, app.take_sfx()) {
            audio.play(sfx);
        }
        profile.record(1, stage.elapsed());

        let stage = Instant::now();
        app.sync(&mut soft, FACE_BUDGET);
        profile.record(2, stage.elapsed());
        profile.record_sync(app.take_sync_profile());

        let stage = Instant::now();
        draws.clear();
        app.draw(&mut draws);
        profile.record_draw_list(stage.elapsed());
        soft.reset_draw_profile();
        let clear_started = Instant::now();
        soft.clear(0);
        profile.record_clear(clear_started.elapsed());
        soft.draw(&draws);
        profile.record(3, stage.elapsed());
        profile.record_draw(soft.take_draw_profile());
        if first_frame {
            eprintln!("slot-onion: first frame rendered in software");
        }
        let stage = Instant::now();
        display.present(&soft.fb)?;
        profile.record(4, stage.elapsed());
        profile.frame();
        if first_frame {
            eprintln!("slot-onion: first frame presented to framebuffer");
            first_frame = false;
        }

        if app.exit().is_some() {
            app.finish(&args.tmp)?;
            return Ok(ExitCode::SUCCESS);
        }

        if let Some(remaining) = FRAME_TIME.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(remaining);
        }
    }
}

#[derive(Default)]
struct FrameProfile {
    frames: u32,
    total_us: [u64; PROFILE_STAGES.len()],
    max_us: [u64; PROFILE_STAGES.len()],
    sync_us: [u64; SYNC_PARTS.len()],
    sync_max_us: [u64; SYNC_PARTS.len()],
    draw_us: [u64; DRAW_PARTS.len()],
    draw_max_us: [u64; DRAW_PARTS.len()],
    draw_calls: [u64; DRAW_PARTS.len()],
    draw_list_us: u64,
    draw_list_max_us: u64,
    clear_us: u64,
    clear_max_us: u64,
    interval_us: u64,
    interval_max_us: u64,
}

impl FrameProfile {
    fn record(&mut self, stage: usize, elapsed: Duration) {
        let micros = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
        self.total_us[stage] = self.total_us[stage].saturating_add(micros);
        self.max_us[stage] = self.max_us[stage].max(micros);
    }

    fn frame(&mut self) {
        self.frames += 1;
        if self.frames < PROFILE_FRAMES {
            return;
        }
        eprint!("slot-onion: frame profile ({} frames)", self.frames);
        for (i, name) in PROFILE_STAGES.iter().enumerate() {
            let avg = self.total_us[i] / u64::from(self.frames);
            eprint!(" {name}={avg}us avg/{}us max", self.max_us[i]);
        }
        for (i, name) in SYNC_PARTS.iter().enumerate() {
            let avg = self.sync_us[i] / u64::from(self.frames);
            eprint!(" {name}={avg}us avg/{}us max", self.sync_max_us[i]);
        }
        for (i, name) in DRAW_PARTS.iter().enumerate() {
            let avg = self.draw_us[i] / u64::from(self.frames);
            eprint!(
                " {name}={avg}us avg/{}us max/{} calls",
                self.draw_max_us[i], self.draw_calls[i]
            );
        }
        eprint!(
            " draw-list={}us avg/{}us max clear={}us avg/{}us max",
            self.draw_list_us / u64::from(self.frames),
            self.draw_list_max_us,
            self.clear_us / u64::from(self.frames),
            self.clear_max_us
        );
        eprint!(
            " frame-interval={}us avg/{}us max",
            self.interval_us / u64::from(self.frames),
            self.interval_max_us
        );
        eprintln!();
        *self = Self::default();
    }

    fn record_sync(&mut self, parts: [u64; SYNC_PARTS.len()]) {
        for (i, micros) in parts.into_iter().enumerate() {
            self.sync_us[i] = self.sync_us[i].saturating_add(micros);
            self.sync_max_us[i] = self.sync_max_us[i].max(micros);
        }
    }

    fn record_draw(&mut self, profile: slot_onion::draw::DrawProfile) {
        for i in 0..DRAW_PARTS.len() {
            self.draw_us[i] = self.draw_us[i].saturating_add(profile.micros[i]);
            self.draw_max_us[i] = self.draw_max_us[i].max(profile.micros[i]);
            self.draw_calls[i] = self.draw_calls[i].saturating_add(profile.calls[i]);
        }
    }

    fn record_draw_list(&mut self, elapsed: Duration) {
        let micros = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
        self.draw_list_us = self.draw_list_us.saturating_add(micros);
        self.draw_list_max_us = self.draw_list_max_us.max(micros);
    }

    fn record_clear(&mut self, elapsed: Duration) {
        let micros = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
        self.clear_us = self.clear_us.saturating_add(micros);
        self.clear_max_us = self.clear_max_us.max(micros);
    }

    fn record_interval(&mut self, elapsed: Duration) {
        let micros = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
        self.interval_us = self.interval_us.saturating_add(micros);
        self.interval_max_us = self.interval_max_us.max(micros);
    }
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

struct Field {
    offset: u32,
    length: u32,
    msb_right: u32,
}

impl Field {
    fn pack(&self, value: u8) -> u32 {
        if self.length == 0 {
            return 0;
        }
        let max = (1u64 << self.length.min(31)) - 1;
        let mut value = ((u64::from(value) * max + 127) / 255) as u32;
        if self.msb_right != 0 {
            value = value.reverse_bits() >> (32 - self.length);
        }
        value << self.offset
    }
}

struct Framebuffer {
    _file: fs::File,
    map: *mut u8,
    map_len: usize,
    width: u32,
    height: u32,
    virtual_width: u32,
    virtual_height: u32,
    pitch: usize,
    xoffset: u32,
    yoffset: u32,
    double_buffer: bool,
    bytes_per_pixel: usize,
    red_lut: [u32; 256],
    green_lut: [u32; 256],
    blue_lut: [u32; 256],
    alpha_lut: [u32; 256],
}

impl Framebuffer {
    fn open(path: &str) -> io::Result<Self> {
        let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        // FBIOGET_VSCREENINFO. The kernel ABI structure starts with the
        // dimensions and four 3-word bitfields; a word buffer keeps its
        // alignment while avoiding a dependency on generated Linux headers.
        let mut info = [0u32; 64];
        let result = unsafe { libc::ioctl(file.as_raw_fd(), 0x4600, info.as_mut_ptr()) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        let width = info[0];
        let height = info[1];
        let virtual_width = info[2].max(width);
        let virtual_height = info[3].max(height);
        let xoffset = info[4];
        let yoffset = info[5];
        let bytes_per_pixel = (info[6] / 8) as usize;
        if width == 0 || height == 0 || !matches!(info[6], 16 | 24 | 32) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported framebuffer format {}x{}x{}",
                    width, height, info[6]
                ),
            ));
        }
        // FBIOGET_FSCREENINFO. The 32-bit ARM ABI puts smem_len at byte 20
        // and line_length at byte 44 in fb_fix_screeninfo.
        let mut fixed = [0u8; 128];
        let result = unsafe { libc::ioctl(file.as_raw_fd(), 0x4602, fixed.as_mut_ptr()) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        let smem_len = u32::from_ne_bytes(fixed[20..24].try_into().unwrap()) as usize;
        let pitch = u32::from_ne_bytes(fixed[44..48].try_into().unwrap()) as usize;
        let min_pitch = usize::try_from(virtual_width)
            .ok()
            .and_then(|w| w.checked_mul(bytes_per_pixel))
            .ok_or_else(|| io::Error::other("framebuffer dimensions overflow"))?;
        if pitch < min_pitch || smem_len == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid framebuffer pitch or memory size",
            ));
        }
        // Some Onion framebuffer drivers expose two virtual pages and support
        // FBIOPAN_DISPLAY. Probe the current page before relying on page flips.
        let double_buffer = virtual_height >= height.saturating_mul(2)
            && unsafe { libc::ioctl(file.as_raw_fd(), 0x4606, info.as_mut_ptr()) } >= 0;
        let map_len = smem_len;
        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if mapped == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        eprintln!(
            "slot-onion: framebuffer {width}x{height}, {} bpp{}",
            info[6],
            if double_buffer {
                ", double-buffered"
            } else {
                ", single-buffered"
            }
        );
        let red = Field {
            offset: info[8],
            length: info[9],
            msb_right: info[10],
        };
        let green = Field {
            offset: info[11],
            length: info[12],
            msb_right: info[13],
        };
        let blue = Field {
            offset: info[14],
            length: info[15],
            msb_right: info[16],
        };
        let alpha = Field {
            offset: info[17],
            length: info[18],
            msb_right: info[19],
        };
        eprintln!("slot-onion: building framebuffer colour tables");
        let lut = |field: &Field| std::array::from_fn(|i| field.pack(i as u8));
        let red_lut = lut(&red);
        let green_lut = lut(&green);
        let blue_lut = lut(&blue);
        let alpha_lut = lut(&alpha);
        eprintln!("slot-onion: framebuffer colour tables ready");
        Ok(Framebuffer {
            _file: file,
            map: mapped.cast(),
            map_len,
            width,
            height,
            virtual_width,
            virtual_height,
            pitch,
            xoffset,
            yoffset,
            double_buffer,
            bytes_per_pixel,
            red_lut,
            green_lut,
            blue_lut,
            alpha_lut,
        })
    }

    fn present(&mut self, pixels: &[u32]) -> io::Result<()> {
        let count = self.width as usize * self.height as usize;
        if pixels.len() < count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "short software framebuffer",
            ));
        }
        let pitch = self.pitch;
        let base_x = self.xoffset as usize * self.bytes_per_pixel;
        let target_yoffset = if self.double_buffer {
            if self.yoffset == 0 { self.height } else { 0 }
        } else {
            self.yoffset
        };
        let base_y = target_yoffset as usize;
        if self
            .xoffset
            .checked_add(self.width)
            .is_none_or(|end| end > self.virtual_width)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "framebuffer x offset is out of bounds",
            ));
        }
        if base_y + self.height as usize > self.virtual_height as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "framebuffer y offset is out of bounds",
            ));
        }
        let last_row_end = (base_y + self.height as usize - 1)
            .checked_mul(pitch)
            .and_then(|offset| offset.checked_add(base_x))
            .and_then(|offset| offset.checked_add(self.width as usize * self.bytes_per_pixel))
            .ok_or_else(|| io::Error::other("framebuffer offset overflow"))?;
        if last_row_end > self.map_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "framebuffer region exceeds mapped memory",
            ));
        }
        for y in 0..self.height as usize {
            // Onion's direct framebuffer is physically rotated 180 degrees.
            // Its graphics blitters apply this transform when presenting.
            let dest_y = base_y + self.height as usize - 1 - y;
            let dest_row = unsafe { self.map.add(dest_y * pitch + base_x) };
            let src_row = &pixels[y * self.width as usize..(y + 1) * self.width as usize];
            // The panel is rotated 180 degrees, so visit source pixels in
            // reverse order to write each destination row contiguously.
            for (dest_x, &pixel) in src_row.iter().rev().enumerate() {
                let r = (pixel >> 16) as u8;
                let g = (pixel >> 8) as u8;
                let b = pixel as u8;
                let packed = self.red_lut[r as usize]
                    | self.green_lut[g as usize]
                    | self.blue_lut[b as usize]
                    | self.alpha_lut[255];
                let dest = unsafe { dest_row.add(dest_x * self.bytes_per_pixel) };
                match self.bytes_per_pixel {
                    2 => unsafe { dest.cast::<u16>().write_unaligned(packed as u16) },
                    3 => unsafe {
                        *dest = packed as u8;
                        *dest.add(1) = (packed >> 8) as u8;
                        *dest.add(2) = (packed >> 16) as u8;
                    },
                    4 => unsafe { dest.cast::<u32>().write_unaligned(packed) },
                    _ => unreachable!(),
                }
            }
        }
        if self.double_buffer {
            let mut info = [0u32; 64];
            if unsafe { libc::ioctl(self._file.as_raw_fd(), 0x4600, info.as_mut_ptr()) } < 0 {
                return Err(io::Error::last_os_error());
            }
            info[5] = target_yoffset;
            if unsafe { libc::ioctl(self._file.as_raw_fd(), 0x4606, info.as_mut_ptr()) } < 0 {
                // Preserve output if this driver advertises virtual pages but
                // rejects a later pan; continue in single-buffer mode.
                self.double_buffer = false;
                return self.present(pixels);
            }
            self.yoffset = target_yoffset;
        }
        Ok(())
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.map.cast(), self.map_len) };
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

struct Evdev {
    file: fs::File,
    pending: Vec<u8>,
}

impl Evdev {
    fn open(path: &str) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self {
            file,
            pending: Vec::new(),
        })
    }

    fn poll(&mut self) -> io::Result<Vec<Event>> {
        let mut ready = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let status = unsafe { libc::poll(&mut ready, 1, 0) };
        if status < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        if status > 0 && ready.revents & libc::POLLIN != 0 {
            let mut bytes = [0u8; 4096];
            loop {
                let read = unsafe {
                    libc::read(
                        self.file.as_raw_fd(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                    )
                };
                if read > 0 {
                    self.pending.extend_from_slice(&bytes[..read as usize]);
                    continue;
                }
                if read == 0 {
                    break;
                }
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if error.kind() != io::ErrorKind::WouldBlock {
                    return Err(error);
                }
                break;
            }
        }

        let event_size = std::mem::size_of::<InputEvent>();
        let complete = self.pending.len() / event_size * event_size;
        let mut events = Vec::new();
        for chunk in self.pending[..complete].chunks_exact(event_size) {
            let raw = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast::<InputEvent>()) };
            if raw.kind != 1 || !(0..=2).contains(&raw.value) {
                continue;
            }
            if let Some(button) = evdev_button(raw.code) {
                events.push(if raw.value == 0 {
                    Event::Release(button)
                } else {
                    Event::Press(button)
                });
            }
        }
        self.pending.drain(..complete);
        Ok(events)
    }
}

// Values match Linux input-event-codes.h and Onion's src/common/system/keymap_hw.h.
fn evdev_button(code: u16) -> Option<Btn> {
    Some(match code {
        103 => Btn::Up,    // KEY_UP
        108 => Btn::Down,  // KEY_DOWN
        105 => Btn::Left,  // KEY_LEFT
        106 => Btn::Right, // KEY_RIGHT
        57 => Btn::A,      // KEY_SPACE
        29 => Btn::B,      // KEY_LEFTCTRL
        42 => Btn::X,      // KEY_LEFTSHIFT
        56 => Btn::Y,      // KEY_LEFTALT
        18 => Btn::L1,     // KEY_E
        20 => Btn::R1,     // KEY_T
        15 => Btn::L2,     // KEY_TAB
        14 => Btn::R2,     // KEY_BACKSPACE
        97 => Btn::Select, // KEY_RIGHTCTRL
        28 => Btn::Start,  // KEY_ENTER
        1 => Btn::Menu,    // KEY_ESC
        _ => return None,
    })
}
