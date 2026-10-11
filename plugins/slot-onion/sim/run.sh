#!/bin/sh
# Plays the part of Onion's runtime.sh main loop on the desktop:
#   frontend -> cmd_to_run.sh -> "game" -> last_game -> frontend (eject) ...
# fake-game stands in for RetroArch and saves a romScreen like keymon does.
#   sim/run.sh [card-dir] [extra frontend args...]
set -u
here=$(cd "$(dirname "$0")" && pwd)
card=${1:-$here/testcard}
[ $# -gt 0 ] && shift
tmp=$here/tmp
bin=$here/../target/release

# Prefer rustup's proxy directly (it honours rust-toolchain.toml) over a
# version-manager shim that may have no Rust selected.
cargo=${CARGO:-cargo}
[ -z "${CARGO:-}" ] && [ -x "$HOME/.cargo/bin/cargo" ] && cargo=$HOME/.cargo/bin/cargo

[ -d "$card" ] || "$here/make-testcard.sh"
(cd "$here/.." && "$cargo" build --release --quiet) || exit 1
mkdir -p "$tmp"
rm -f "$tmp/cmd_to_run.sh" "$tmp/last_game" "$tmp/use_stock_mainui"

while true; do
    "$bin/slot-onion" --sdcard "$card" --tmp "$tmp" "$@"
    code=$?
    rm -f "$tmp/last_game"

    if [ -f "$tmp/use_stock_mainui" ]; then
        rm -f "$tmp/use_stock_mainui"
        echo "[runtime] stock MainUI would open now. Enter to return to the frontend, Ctrl-C to stop."
        read -r _ || exit 0
        continue
    fi
    if [ ! -f "$tmp/cmd_to_run.sh" ]; then
        echo "[runtime] frontend exited ($code) without a command; on device this falls back to MainUI"
        exit 0
    fi

    cmd=$(cat "$tmp/cmd_to_run.sh")
    rm -f "$tmp/cmd_to_run.sh"
    echo "[runtime] $cmd"
    # Same extraction as runtime.sh's launch_game: the second quoted argument.
    rompath=$(echo "$cmd" | awk '{ st = index($0,"\" \""); print substr($0,st+3,length($0)-st-3)}')
    "$bin/fake-game" --sdcard "$card" --rom "$rompath" "$@"
    # runtime.sh passes the realpath'd rompath to the post-game hook.
    echo "$rompath" | sed -E -e ':a' -e 's#/[^/]+/\.\./#/#' -e 'ta' > "$tmp/last_game"
done
