#!/bin/sh
set -eu
: "${BRICK_TOOLCHAIN:?Set BRICK_TOOLCHAIN to the extracted TrimUI toolchain directory}"
cd "$(dirname "$0")/.."
export PATH="$BRICK_TOOLCHAIN/bin:$PATH"
MGBA=e31759b24e7a4e3899285ff720d7b573ac328ae7
GPSP=7b2dd37394e2822110cacef321e9bb291a305b77
FCEUMM=7a542dab1e87679921962a9f056186eca425c0c2
SNES9X=fae2fea08f74180759ef540ee94259213f503480
PICODRIVE=1890c2932234c9d30f4cd3851d02228baae8f09e
BEETLE_NGP=a50d5ac288a81f2104ddf43195a4efdd15c72227
BEETLE_WSWAN=4b01295838ea89e3f1355bbe4cb5cf98aa6108cd
mkdir -p vendor brick/core-src
[ -f vendor/mgba-src.tar.gz ] || curl -fL "https://github.com/libretro/mgba/archive/$MGBA.tar.gz" -o vendor/mgba-src.tar.gz
[ -f vendor/gpsp-src.tar.gz ] || curl -fL "https://github.com/libretro/gpsp/archive/$GPSP.tar.gz" -o vendor/gpsp-src.tar.gz
[ -f vendor/fceumm-src.tar.gz ] || curl -fL "https://github.com/libretro/libretro-fceumm/archive/$FCEUMM.tar.gz" -o vendor/fceumm-src.tar.gz
[ -f vendor/snes9x-src.tar.gz ] || curl -fL "https://github.com/libretro/snes9x/archive/$SNES9X.tar.gz" -o vendor/snes9x-src.tar.gz
# The NES and SNES archives are committed; a fresh download must match them byte for byte.
sha256sum -c - <<'SUMS'
f80b5c1df39e22d78791b1867ea79f3ec8c0823e4b1f82b3f89e432c614a6c53  vendor/fceumm-src.tar.gz
0d4b0c4181d66668ec0040eb17f90a559f7b7fa6cbd9198593c3bdbe79acd029  vendor/snes9x-src.tar.gz
SUMS
mgba_src="$PWD/brick/core-src/mgba-$MGBA"
gpsp_src="$PWD/brick/core-src/gpsp-$GPSP"
fceumm_src="$PWD/brick/core-src/libretro-fceumm-$FCEUMM"
snes9x_src="$PWD/brick/core-src/snes9x-$SNES9X"
if [ ! -d "$mgba_src" ]; then
    tar --no-same-owner -xzf vendor/mgba-src.tar.gz -C brick/core-src
    # Its own repository, so git apply works relative to the core source and not this checkout.
    git -C "$mgba_src" init -q
    for p in cores/mgba/*.patch; do git -C "$mgba_src" apply "$PWD/$p"; done
fi
if [ ! -d "$gpsp_src" ]; then
    tar --no-same-owner -xzf vendor/gpsp-src.tar.gz -C brick/core-src
    git -C "$gpsp_src" init -q
    for p in cores/gpsp/*.patch brick/gpsp-old-binutils.patch; do git -C "$gpsp_src" apply "$PWD/$p"; done
fi
[ -d "$fceumm_src" ] || tar --no-same-owner -xzf vendor/fceumm-src.tar.gz -C brick/core-src
[ -d "$snes9x_src" ] || tar --no-same-owner -xzf vendor/snes9x-src.tar.gz -C brick/core-src
# PicoDrive needs its submodules (the 68000 core, libchdr, emu2413), which an archive leaves out.
picodrive_src="$PWD/brick/core-src/picodrive-$PICODRIVE"
if [ ! -d "$picodrive_src" ]; then
    git clone -q https://github.com/libretro/picodrive "$picodrive_src"
    git -C "$picodrive_src" checkout -q "$PICODRIVE"
    git -C "$picodrive_src" submodule update -q --init --recursive
fi
ngp_src="$PWD/brick/core-src/beetle-ngp-$BEETLE_NGP"
if [ ! -d "$ngp_src" ]; then
    git clone -q https://github.com/libretro/beetle-ngp-libretro "$ngp_src"
    git -C "$ngp_src" checkout -q "$BEETLE_NGP"
fi
wswan_src="$PWD/brick/core-src/beetle-wswan-$BEETLE_WSWAN"
if [ ! -d "$wswan_src" ]; then
    git clone -q https://github.com/libretro/beetle-wswan-libretro "$wswan_src"
    git -C "$wswan_src" checkout -q "$BEETLE_WSWAN"
fi
cmake -S "$mgba_src" -B brick/core-build/mgba \
    -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_SYSTEM_PROCESSOR=aarch64 \
    -DCMAKE_C_COMPILER=aarch64-linux-gnu-gcc -DCMAKE_CXX_COMPILER=aarch64-linux-gnu-g++ \
    -DLIBMGBA_ONLY=ON -DBUILD_LIBRETRO=ON -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_C_FLAGS=-mcpu=cortex-a53 \
    -DCMAKE_DISABLE_FIND_PACKAGE_PNG=ON -DCMAKE_DISABLE_FIND_PACKAGE_ZLIB=ON \
    -DCMAKE_DISABLE_FIND_PACKAGE_LibZip=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build brick/core-build/mgba --target mgba_libretro --parallel 8
make -C "$gpsp_src" platform=arm64 CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# rpi3_64 is FCEUmm's Cortex-A53 target, the Brick's CPU.
make -C "$fceumm_src" -f Makefile.libretro platform=rpi3_64 CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# snes9x is C++: its runtime is linked in, as the firmware's libstdc++ may be too old or absent.
CXXFLAGS="-mcpu=cortex-a53 -mtune=cortex-a53" LDFLAGS="-static-libstdc++ -static-libgcc" \
    make -C "$snes9x_src/libretro" platform=unix CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# PicoDrive: Game Gear and Master System (later Mega Drive, Sega CD, 32X), with its SH2
# recompiler for the 32X. Its own CFLAGS add the include paths, so ours go in the environment.
CFLAGS="-mcpu=cortex-a53" make -C "$picodrive_src" -f Makefile.libretro platform=aarch64 CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# Beetle NeoPop: Neo Geo Pocket and Color. C++, so its runtime is linked in, as for snes9x.
CFLAGS="-mcpu=cortex-a53" CXXFLAGS="-mcpu=cortex-a53" LDFLAGS="-static-libstdc++ -static-libgcc" \
    make -C "$ngp_src" platform=unix CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# Beetle Cygne: WonderSwan and Color, built the same way.
CFLAGS="-mcpu=cortex-a53" CXXFLAGS="-mcpu=cortex-a53" LDFLAGS="-static-libstdc++ -static-libgcc" \
    make -C "$wswan_src" platform=unix CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
cp brick/core-build/mgba/mgba_libretro.so vendor/
cp "$gpsp_src/gpsp_libretro.so" vendor/
cp "$fceumm_src/fceumm_libretro.so" vendor/
cp "$snes9x_src/libretro/snes9x_libretro.so" vendor/
cp "$picodrive_src/picodrive_libretro.so" vendor/
cp "$ngp_src/mednafen_ngp_libretro.so" vendor/
cp "$wswan_src/mednafen_wswan_libretro.so" vendor/
