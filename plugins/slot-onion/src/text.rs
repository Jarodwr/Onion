use slot_ui::text::{coverage, fit, label_font, line_width};

use crate::draw::Face;

/// One line of slot's label lettering (uppercased), tightly sized.
pub fn text_face(text: &str, px: f32, colour: [u8; 3], max_w: f32) -> Option<Face> {
    let font = label_font()?;
    let layout = fit(font, text, max_w, 1, px, px * 0.6);
    let line = layout.lines.first()?;
    let w = line_width(font, line, layout.px, layout.tracking).ceil() as u32 + 4;
    let h = (layout.px * 1.5).ceil() as u32;
    let rgba = coverage(w, h, &layout)
        .into_iter()
        .flat_map(|a| [colour[0], colour[1], colour[2], a])
        .collect();
    Some(Face { rgba, w, h })
}
