pub type Millis = u64;

/// Miyoo Mini buttons. The desktop maps keys onto these; the device reads
/// them from evdev.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Btn {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    Menu,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Event {
    Press(Btn),
    Release(Btn),
}
