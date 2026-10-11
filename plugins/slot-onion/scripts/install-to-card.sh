#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
card=${1:?Usage: scripts/install-to-card.sh /path/to/SDCARD}
card=$(CDPATH= cd -- "$card" && pwd)
sysdir="$card/.tmp_update"
binary="$repo/package/.tmp_update/bin/slot-onion"
launcher="$repo/package/.tmp_update/config/frontend/launch.sh"

if [ ! -f "$sysdir/runtime.sh" ]; then
    echo "No Onion runtime at $sysdir/runtime.sh; select the root of the Onion SD card." >&2
    exit 1
fi
if ! grep -q 'custom_frontend_enabled' "$sysdir/runtime.sh"; then
    echo "This Onion install does not have the custom frontend hook. Update it with the frontend-hooks build first." >&2
    exit 1
fi
if [ ! -x "$binary" ]; then
    echo "Device binary not found. Run scripts/build-miyoo.sh first." >&2
    exit 1
fi

mkdir -p "$sysdir/bin" "$sysdir/config/frontend" "$card/Labels" "$card/Config"

# Slot label art lives beside Onion's labels. Create one directory for every
# configured Onion system, plus the systems with native Slot cart renderers.
for system in GB GBC GBA; do
    mkdir -p "$card/Labels/$system"
done
for config in "$card"/Emu/*/config.json; do
    [ -f "$config" ] || continue
    system=$(basename "$(dirname "$config")")
    mkdir -p "$card/Labels/$system"
done

# Seed Slot's per-game shell files only on first install. Existing choices are
# user data and must survive frontend upgrades.
if [ ! -e "$card/Config/cart_shell.txt" ]; then
    cat > "$card/Config/cart_shell.txt" <<'EOF'
# Optional per-game cartridge shell: ROM name = outline RRGGBB finish
# Example: Pokemon Blue = auto 2b3a88 solid
EOF
fi
if [ ! -e "$card/Labels/cart_shell.txt" ]; then
    cat > "$card/Labels/cart_shell.txt" <<'EOF'
# Optional shell overrides for games with custom label artwork.
# Format: ROM name = outline RRGGBB finish
EOF
fi

if [ -f "$sysdir/config/frontend/launch.sh" ] \
    && [ ! -f "$sysdir/config/frontend/launch.sh.bak" ]; then
    cp "$sysdir/config/frontend/launch.sh" "$sysdir/config/frontend/launch.sh.bak"
fi
install -m 755 "$binary" "$sysdir/bin/slot-onion"
install -m 755 "$launcher" "$sysdir/config/frontend/launch.sh"
echo "Installed Slot-Onion frontend to $card"
