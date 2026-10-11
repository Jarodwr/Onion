/// Logical canvas width. slot lays everything out on a 720px-wide canvas;
/// the height follows the panel's aspect so a 4:3 panel gets 720x540.
pub const CANVAS_W: f32 = 720.0;

#[derive(Copy, Clone, Debug)]
pub struct Canvas {
    pub w: f32,
    pub h: f32,
}

impl Canvas {
    pub fn for_panel(panel_w: u32, panel_h: u32) -> Self {
        Canvas {
            w: CANVAS_W,
            h: (CANVAS_W * panel_h as f32 / panel_w as f32).round(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TexId(pub usize);

/// RGBA8 straight-alpha image.
pub struct Face {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextureKind {
    Cart,
    Shadow,
    Chrome,
    Ui,
    Other,
}

#[derive(Default, Copy, Clone)]
pub struct DrawProfile {
    pub micros: [u64; 6],
    pub calls: [u64; 6],
}

/// The subset of slot's draw list this frontend needs, in canvas coordinates.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Draw {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        colour: [f32; 4],
    },
    Tex {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
    },
}

pub trait Textures {
    fn create_texture(&mut self, face: &Face, kind: TextureKind) -> TexId;
}

pub fn rgb(c: [u8; 3]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        1.0,
    ]
}
