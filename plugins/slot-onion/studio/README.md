# Slot Cart Studio for Onion

Cart labels for Onion SD cards, previewed on the same GB, GBC and GBA cartridge faces Slot-Onion
draws. The Jarodwr fork publishes the Studio at
[jarodwr.github.io/Onion/slot-studio](https://jarodwr.github.io/Onion/slot-studio/).

Choose the root of the Onion SD card. Studio reads each supported system's `Emu/<system>/config.json`
and follows its `rompath` into `Roms/`; it writes labels into `Labels/<system>/` and shell choices
to `Labels/cart_shell.txt`. Browsers without direct card access download a ZIP to copy onto the card.
Other Onion systems are ignored because this renderer currently has cartridge faces only for GB,
GBC and GBA.

## Local development

From the Onion repository root, run `mise install`, then:

```sh
mise run studio-build
mise run studio-serve
```

Open `http://127.0.0.1:8765`. The workspace vendors the exact Slot renderer source used by Studio
under `slot/`; `slot.ref` records its upstream revision.

## License

GPL-3.0-or-later, same as slot. See [LICENSE](LICENSE).

Game names and CRCs come from
[libretro-database](https://github.com/libretro/libretro-database) (CC BY-SA 4.0); logos come
from [ScreenScraper](https://www.screenscraper.fr/) (CC BY-NC-SA 4.0).

Game Boy is a trademark of Nintendo. Not affiliated with Nintendo.
