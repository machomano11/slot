#!/bin/sh
set -eu
: "${BRICK_TOOLCHAIN:?Set BRICK_TOOLCHAIN}"
cd "$(dirname "$0")/.."
OUT="${1:-dist-brick}"
if [ -e "$OUT" ]; then echo "Output already exists: $OUT. Choose a new directory." >&2; exit 1; fi
mkdir -p "$OUT"
cp -R brick/card/. "$OUT/"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$BRICK_TOOLCHAIN/bin/aarch64-linux-gnu-gcc"

# Boot Logo app: picture converter plus its own copy of the file checker.
( cd brick/bootlogo-tool && cargo build --locked --release --target aarch64-unknown-linux-gnu )
cp brick/bootlogo-tool/target/aarch64-unknown-linux-gnu/release/slot-bootlogo "$OUT/Apps/BootLogo/"
cp brick/card/Slot/System/slot-filecheck "$OUT/Apps/BootLogo/"
cp target/aarch64-unknown-linux-gnu/device/slot "$OUT/Slot/System/"
cp vendor/mgba_libretro.so vendor/gpsp_libretro.so vendor/fceumm_libretro.so vendor/snes9x_libretro.so vendor/picodrive_libretro.so vendor/mednafen_ngp_libretro.so vendor/mednafen_wswan_libretro.so "$OUT/Slot/System/"
for f in slot mgba_libretro.so gpsp_libretro.so fceumm_libretro.so snes9x_libretro.so picodrive_libretro.so mednafen_ngp_libretro.so mednafen_wswan_libretro.so; do "$BRICK_TOOLCHAIN/bin/aarch64-linux-gnu-strip" "$OUT/Slot/System/$f"; done
for d in BIOS Config Games/GB Games/GBC Games/GBA Games/NES Games/SNES Games/GG Games/SMS Games/MD Games/NGP Games/NGPC Games/WS Games/WSC Games/SEGACD Labels/GB Labels/GBC Labels/GBA Labels/NES Labels/SNES Labels/GG Labels/SMS Labels/MD Labels/NGP Labels/NGPC Labels/WS Labels/WSC Labels/SEGACD Saves/GB Saves/GBC Saves/GBA Saves/NES Saves/SNES Saves/GG Saves/SMS Saves/MD Saves/NGP Saves/NGPC Saves/WS Saves/WSC Saves/SEGACD States/GB States/GBC States/GBA States/NES States/SNES States/GG States/SMS States/MD States/NGP States/NGPC States/WS States/WSC States/SEGACD Wallpapers Logs; do mkdir -p "$OUT/Slot/$d"; done
cp brick/README.md "$OUT/Slot/README.md"
# Spare copy of the hidden boot script: install-autoboot.sh restores it when a
# Mac copy left .slot-boot behind.
cp brick/card/.slot-boot/boot.sh "$OUT/Slot/System/slot-boot.sh"
cp -R licenses "$OUT/Slot/System/"
cp LICENSE "$OUT/Slot/System/licenses/slot-GPL-3.0.txt"
chmod +x "$OUT/Slot/System/slot" "$OUT/Slot/System/"*.sh "$OUT/Apps/"*/launch.sh "$OUT/Apps/BootLogo/slot-bootlogo" "$OUT/Apps/BootLogo/slot-filecheck"
echo "Card files written to $OUT"
