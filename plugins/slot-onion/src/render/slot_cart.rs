use slot_store::{Cart, Platform};
use slot_ui::{cart_box, cart_face, cart_shadow, gb_cart_shadow, gb_shell_of, seated_box};

use super::CartRenderer;
use crate::card::{Card, Game};
use crate::draw::Face;

/// slot's own procedurally drawn GB, GBC and GBA carts.
pub struct SlotCart(pub Platform);

impl SlotCart {
    fn cart(&self, card: &Card, game: &Game) -> Cart {
        Cart {
            platform: self.0,
            stem: game.stem.clone(),
            rom: card.host(&game.rom),
            label: game.label.as_deref().map(|p| card.host(p)),
            title: String::new(),
            code: String::new(),
            shell: game.shell,
        }
    }
}

impl CartRenderer for SlotCart {
    fn size(&self, _: &Game) -> (u32, u32) {
        cart_box(self.0)
    }

    fn seated(&self, _: &Game) -> (u32, u32) {
        seated_box(self.0)
    }

    fn face(&self, card: &Card, game: &Game) -> Face {
        let f = cart_face(&self.cart(card, game));
        Face {
            rgba: f.rgba,
            w: f.w,
            h: f.h,
        }
    }

    fn shadow(&self, card: &Card, game: &Game) -> Option<(String, Face)> {
        let (key, f) = match gb_shell_of(&self.cart(card, game)) {
            None => ("slot-gba".to_owned(), cart_shadow()),
            Some(shell) => (format!("slot-gb-{shell:?}"), gb_cart_shadow(shell)),
        };
        Some((
            key,
            Face {
                rgba: f.rgba,
                w: f.w,
                h: f.h,
            },
        ))
    }
}
