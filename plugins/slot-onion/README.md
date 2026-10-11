# slot-onion

The cart shelf and cartridge insert/eject animation from
[slot](https://github.com/BrandonKowalski/slot), as a frontend for Onion OS.
Targets the Miyoo Mini Plus (640x480); other panels via `--panel`. GPL-3.0.

Needs the frontend hooks in Onion's `runtime.sh` (Onion fork, `frontend-hooks` branch).

This is a regular directory in the Onion repository; it is not a Git submodule. Its Slot crates
are vendored under `upstream-slot/` and Cargo uses local paths, so the plugin build does not fetch
Slot source from Git. `slot/` is the separate Slot snapshot pinned by Cart Studio's
`slot.ref`; keeping the two revisions separate preserves the renderer Studio was designed against.
Both include their upstream license files.

From the Onion repository root, `mise run slot-onion-build` builds the desktop plugin,
`mise run slot-onion-device` cross-compiles the Miyoo binary, and `mise run studio-build` builds
Cart Studio's web renderer. The Onion Pages workflow publishes the Studio at
`/slot-studio/` along with the documentation site.

## Desktop development

Uses Rust 1.98.1 (`rust-toolchain.toml`).
The matching `mise.toml` keeps mise shims on that version; run `mise trust`
once in this checkout if mise asks to trust the project config.

```sh
sim/make-testcard.sh   # builds sim/testcard from ../Onion's Emu configs + placeholder ROMs
sim/run.sh             # plays runtime.sh's loop: shelf -> fake game -> eject -> shelf
cargo run --release -- --sdcard sim/testcard --tmp sim/tmp   # just the frontend
cargo run --release --example frames -- sim/testcard out/    # dump key frames to PNGs
```

The window shows the 640x480 framebuffer one pixel per point; pass `--zoom 2`
to enlarge it, or `--zoom 0.5` on a Retina display for pixel-exact.

`fake-game` stands in for RetroArch. Esc leaves the "game" and saves a
romScreen named the way keymon names it, so the eject animation uses it.

Keys: arrows = d-pad, X = A, Z = B, Q/W = L1/R1, Backspace = Select, Esc = quit.
On the shelf: left/right picks a cart, L1/R1 switches system, and A inserts.
Hold A for 0.5 seconds to start fresh without loading Onion's auto-save state.
Y or Select opens stock MainUI. Games launch through their Onion system launch
script, so Onion remains responsible for applying per-cartridge settings such
as GLO's selected core.

When Slot Onion is installed as the custom frontend, Y or Select opens stock
MainUI for the current session, focused on the selected cart in Recents. Press
Y there to return directly to Slot without launching a game.

## Cartridge labels and shells

Slot-Onion looks for dedicated label artwork at
`Labels/<system>/<ROM filename>.png` (for example,
`Labels/GBA/Pokemon Emerald.png`). This takes priority over Onion's configured
`imgpath` box art. If neither exists, Slot draws a generated title label.

Per-game cartridge shells use Slot's config format in
`Config/cart_shell.txt`:

```text
Pokemon Emerald = auto 2f8f4e solid
Pokemon Blue = auto 2b3a88 solid
```

Each line is `<ROM filename without extension> = <outline> <RRGGBB> <finish>`.
Outlines are `auto`, `notched`, or `rounded`; finishes are `solid`, `clear`, or
`glitter`. Entries in `Labels/cart_shell.txt` override the matching entries in
`Config/cart_shell.txt`; `auto` there removes the override. GB, GBC, and GBA
renderers use the full shell choice. Other systems use the configured color on
their generic cart. The SD installer creates the system label folders and both
shell config files when missing, and preserves existing shell settings.

## Layout

- `card.rs`: Onion's card as MainUI sees it (Emu configs, ROMs, recents, romScreens).
- `render/`: cart renderers per system. `slot_cart.rs` uses slot's GB/GBC/GBA
  carts, and `generic.rs` is the fallback. Register new ones in `render/mod.rs`.
- `shelf.rs`, `chrome.rs`: slot's shelf and cartridge slot, generic over cart size.
- `app.rs`: shelf -> insert -> hand off, or eject -> shelf.
- `soft.rs`: CPU rasteriser (the Mini has no usable GPU).

## Frontend contract (runtime.sh)

`.tmp_update/config/frontend/launch.sh` is run in place of MainUI. It should:
- write a command to `/tmp/cmd_to_run.sh` and exit 0 to launch something, or
- `touch /tmp/use_stock_mainui` and exit to open stock MainUI once.

`/tmp/last_game` holds the realpath of the game that just exited. Holding MENU
at boot, or a non-zero exit with no command written, falls back to stock MainUI
until reboot.

## Miyoo device build and install

The device binary targets ARMv7 hard-float and uses Onion's `/dev/fb0` framebuffer
and `/dev/input/event0` button device. It rotates framebuffer output to match
Onion's direct framebuffer convention and sends audio cues through Onion's
OSS `/dev/dsp` compatibility layer.
Docker must be running; on Apple Silicon the build scripts run Onion's amd64
toolchain image under emulation.

```sh
scripts/build-miyoo.sh
scripts/install-to-card.sh /Volumes/ONION
```

Replace `/Volumes/ONION` with the mounted root of the SD card. The install
script keeps a backup of an existing custom frontend launcher at
`.tmp_update/config/frontend/launch.sh.bak`. The SD card must already have the
Onion `frontend-hooks` runtime installed. To remove Slot-Onion, restore that
backup or remove `.tmp_update/config/frontend/launch.sh`.

The desktop build remains available with `cargo run --release -- ...`.
Framebuffer formats and controls are implemented for the Miyoo Linux ABI;
performance and input/audio behavior still need validation on physical
hardware.
