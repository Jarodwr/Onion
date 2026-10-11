//! Cart renderers, one per Onion system, with a generic fallback.
//!
//! To give a system its own cart, implement `CartRenderer` in a new module and
//! register it under the system's Emu folder name in `Registry::new`.

mod generic;
mod slot_cart;

use std::collections::HashMap;

use slot_store::Platform;

use crate::card::{Card, Game};
use crate::draw::Face;

pub use generic::GenericCart;
pub use slot_cart::SlotCart;

pub trait CartRenderer {
    /// Size on the shelf, in canvas pixels, for the centre (unshrunk) slot.
    fn size(&self, game: &Game) -> (u32, u32);
    /// Size once pushed into the slot mouth. Width should match `SEATED_W`
    /// so every cart fits the same mouth.
    fn seated(&self, game: &Game) -> (u32, u32);
    /// The cart artwork at `size`.
    fn face(&self, card: &Card, game: &Game) -> Face;
    /// A solid silhouette drawn behind faded side carts, with a key so carts
    /// that share a shape share the texture.
    fn shadow(&self, card: &Card, game: &Game) -> Option<(String, Face)>;
}

pub struct Registry {
    by_system: HashMap<String, Box<dyn CartRenderer>>,
    fallback: Box<dyn CartRenderer>,
}

impl Registry {
    pub fn new() -> Self {
        let mut by_system: HashMap<String, Box<dyn CartRenderer>> = HashMap::new();
        by_system.insert("GB".into(), Box::new(SlotCart(Platform::Gb)));
        by_system.insert("GBC".into(), Box::new(SlotCart(Platform::Gbc)));
        by_system.insert("GBA".into(), Box::new(SlotCart(Platform::Gba)));
        Registry {
            by_system,
            fallback: Box::new(GenericCart),
        }
    }

    pub fn get(&self, system: &str) -> &dyn CartRenderer {
        self.by_system
            .get(system)
            .map_or(self.fallback.as_ref(), Box::as_ref)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
