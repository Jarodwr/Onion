#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
image=slot-onion-miyoo-builder

docker build --platform linux/amd64 -f "$repo/Dockerfile.miyoo" -t "$image" "$repo"
docker run --rm --platform linux/amd64 \
    -v "$repo:/work/slot-onion" \
    -w /work/slot-onion \
    "$image" /bin/bash -lc '
        set -e
        source /root/.bashrc 2>/dev/null || true
        cross_gcc=$(command -v arm-linux-gnueabihf-gcc || true)
        if [ -z "$cross_gcc" ]; then
            cross_gcc=$(find /opt /usr/local/bin /usr/bin -type f \
                -name arm-linux-gnueabihf-gcc -print -quit 2>/dev/null || true)
        fi
        if [ -z "$cross_gcc" ]; then
            echo "Onion ARM compiler arm-linux-gnueabihf-gcc is missing from the toolchain image." >&2
            exit 1
        fi
        export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER="$cross_gcc"
        export CC_armv7_unknown_linux_gnueabihf="$cross_gcc"
        cargo build --release --locked --no-default-features --features device \
            --bin slot-onion-device --target armv7-unknown-linux-gnueabihf
        install -D -m 755 \
            target/armv7-unknown-linux-gnueabihf/release/slot-onion-device \
            package/.tmp_update/bin/slot-onion
        cross_strip=${cross_gcc%gcc}strip
        if [ -x "$cross_strip" ]; then
            "$cross_strip" package/.tmp_update/bin/slot-onion
        else
            echo "Cross-strip not available; leaving the device binary unstripped."
        fi
    '

echo "Device frontend staged under $repo/package/.tmp_update"
