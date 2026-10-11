#!/bin/sh
# Builds a small Onion SD card tree for desktop testing, using the real Emu
# configs from an Onion checkout and placeholder ROM files.
#   sim/make-testcard.sh [onion-checkout] [out-dir]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
onion=${1:-$here/../../Onion}
card=${2:-$here/testcard}
packages="$onion/static/packages/Emu"

[ -d "$packages" ] || { echo "no Onion checkout at $onion" >&2; exit 1; }

emu() { # <package dir> <system>
    mkdir -p "$card/Emu/$2" "$card/Roms/$2/Imgs"
    cp "$packages/$1/Emu/$2/config.json" "$packages/$1/Emu/$2/launch.sh" "$card/Emu/$2/"
}
rom() { # <system> <file>
    : > "$card/Roms/$1/$2"
}
gba_rom() { # <filename> <game code>
    file="$card/Roms/GBA/$1"
    dd if=/dev/zero of="$file" bs=1 count=176 2>/dev/null
    printf '%s' "$2" | dd of="$file" bs=1 seek=172 conv=notrunc 2>/dev/null
}
gb_rom() { # <system> <filename> <header title> <CGB flag> [game code]
    file="$card/Roms/$1/$2"
    dd if=/dev/zero of="$file" bs=1 count=336 2>/dev/null
    printf '%s' "$3" | dd of="$file" bs=1 seek=308 conv=notrunc 2>/dev/null
    if [ "$#" -ge 5 ]; then
        printf '%s' "$5" | dd of="$file" bs=1 seek=319 conv=notrunc 2>/dev/null
    fi
    case "$4" in
        128) printf '\200' | dd of="$file" bs=1 seek=323 conv=notrunc 2>/dev/null ;;
        192) printf '\300' | dd of="$file" bs=1 seek=323 conv=notrunc 2>/dev/null ;;
    esac
    printf '\001' | dd of="$file" bs=1 seek=330 conv=notrunc 2>/dev/null
}

rm -rf "$card"
emu "Nintendo - GBA (gpSP)" GBA
emu "Nintendo - Game Boy (Gambatte)" GB
emu "Nintendo - Game Boy Color (Gambatte)" GBC
emu "Nintendo - SNES (Beetle Supafaust)" SFC
emu "Sega - Genesis (PicoDrive)" MD

# GBA header codes exercise slot's known cartridge shells (red, blue, green,
# orange and clear). These are header-only placeholders, not playable ROMs.
gba_rom "Advance Wars (USA).gba" "AXV"
gba_rom "Golden Sun (USA, Europe).gba" "AXP"
gba_rom "Metroid Fusion (USA).gba" "BPE"
gba_rom "Mother 3 (Japan).gba" "BPR"
gba_rom "Pokemon - Emerald Version (USA, Europe).gba" "BPG"
gba_rom "Clear Shell Sample.gba" "U3I"
gba_rom "Clear Shell Sample 2.gba" "U32"
gba_rom "Clear Shell Sample 3.gba" "U33"
gba_rom "Yoshi Sample.gba" "KYGE"
gba_rom "Yoshi Sample 2.gba" "KYGP"

# GB headers vary between original, dual-mode and colour-only shells. Pokemon
# title/code samples also exercise slot's game-specific shell mappings.
gb_rom GB "Tetris (World) (Rev 1).gb" "TETRIS" 0
gb_rom GB "Kirby's Dream Land (USA, Europe).gb" "KIRBY DREAM" 0
gb_rom GB "Super Mario Land (World).gb" "MARIO LAND" 0
gb_rom GB "Pokemon Red (USA).gb" "POKEMON RED" 0
gb_rom GB "Pokemon Blue (USA).gb" "POKEMON BLU" 0
gb_rom GB "Pokemon Yellow (USA).gb" "POKEMON YEL" 0
gb_rom GB "Dual Mode Shell Sample.gb" "DUAL SAMPLE" 128
gb_rom GB "Colour Shell Sample.gb" "COLOR SAMPLE" 192

gb_rom GBC "Link's Awakening DX (USA).gbc" "ZELDA DX" 128
gb_rom GBC "Pokemon Crystal (USA).gbc" "POKEMON CRY" 192 "BYT"
gb_rom GBC "Clear Shell Sample.gbc" "CLEAR SAMPLE" 192 "KCE"
gb_rom GBC "Dual Mode Shell Sample.gbc" "DUAL SAMPLE" 128 "BMG"
gb_rom GBC "Shantae Sample.gbc" "SHANTAE" 192 "VCA"
gb_rom GBC "Dragon Warrior III Sample.gbc" "DRAGON III" 192 "VPHJ"
gb_rom GBC "Wario Land 3 Sample.gbc" "WARIO LAND3" 192 "KTN"

for f in "Chrono Trigger (USA).sfc" "Super Metroid (USA).sfc" "EarthBound (USA).sfc" \
         "A Link to the Past (USA).sfc" "Donkey Kong Country (USA).sfc"; do rom SFC "$f"; done
for f in "Sonic The Hedgehog (USA, Europe).md" "Streets of Rage 2 (USA).md" \
         "Gunstar Heroes (USA).md" "Phantasy Star IV (USA).md"; do rom MD "$f"; done

mkdir -p "$card/Saves/CurrentProfile/romScreens"
echo "test card at $card"
