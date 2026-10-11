//! Desktop stand-ins for the device: a window as the framebuffer and the
//! keyboard as the Mini's buttons.

use minifb::{Key, ScaleMode, Window, WindowOptions};

pub use crate::args::Args;
use crate::input::Btn;

pub const KEY_HELP: &str = "keys: arrows = d-pad, X = A, Z = B, S = X, A = Y, \
                            Q/W = L1/R1, Enter = Start, Backspace = Select, Esc = Menu; \
                            hold X for 0.5s to launch fresh; Y or Select opens MainUI, \
                            Y returns to Slot from MainUI";

/// One panel pixel per window point: pixel-exact on a 1x display. On a
/// Retina display a point is 2x2 pixels, so use `--zoom 0.5` there for
/// pixel-exact (and roughly the Mini Plus's real 3.5" size).
pub fn window(title: &str, args: &Args) -> Result<Window, minifb::Error> {
    let size = |px: u32| ((px as f32 * args.zoom).round() as usize).max(1);
    let mut window = Window::new(
        title,
        size(args.panel.0),
        size(args.panel.1),
        WindowOptions {
            scale_mode: ScaleMode::Stretch,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(60);
    Ok(window)
}

pub fn button(key: Key) -> Option<Btn> {
    Some(match key {
        Key::Up => Btn::Up,
        Key::Down => Btn::Down,
        Key::Left => Btn::Left,
        Key::Right => Btn::Right,
        Key::X => Btn::A,
        Key::Z => Btn::B,
        Key::S => Btn::X,
        Key::A => Btn::Y,
        Key::Q => Btn::L1,
        Key::W => Btn::R1,
        Key::Key1 => Btn::L2,
        Key::Key2 => Btn::R2,
        Key::Enter => Btn::Start,
        Key::Backspace => Btn::Select,
        Key::Escape => Btn::Menu,
        _ => return None,
    })
}
