//! slot's cart shelf as an Onion OS frontend.
//!
//! The shelf, slot chrome and insert/eject timing are adapted from
//! slot (https://github.com/BrandonKowalski/slot, GPL-3.0); cart artwork for
//! the Game Boy family comes straight from `slot-ui`.

pub mod app;
pub mod args;
#[cfg(any(feature = "host", feature = "device"))]
pub mod audio;
pub mod card;
pub mod chrome;
pub mod draw;
pub mod hash;
#[cfg(feature = "host")]
pub mod host;
pub mod input;
pub mod render;
pub mod shelf;
pub mod soft;
pub mod text;
