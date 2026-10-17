#!/usr/bin/env python3
"""Regenerate card/Slot/System/Boxarts/<console>.txt from the libretro-thumbnails repositories.

Cart Art falls back to a game's front cover where it has no label scan or cart picture (and
for the consoles that have neither): the cover is what the cart's label showed, near enough.
The covers are libretro's (github.com/libretro-thumbnails), named as No-Intro and Redump
name the games. This lists the names each repository has, so the Brick matches a ROM to its
cover without asking GitHub for listings. Only the file list is fetched (a blobless clone),
never the pictures.

Names are written as they are in the repository, without .png; only names without control
characters are kept, and the Brick percent-encodes them before asking for one.

Usage: python3 brick/make-boxart-names.py
"""
import pathlib, subprocess, sys, tempfile

REPOS = {
    'GB': 'Nintendo_-_Game_Boy',
    'GBC': 'Nintendo_-_Game_Boy_Color',
    'GBA': 'Nintendo_-_Game_Boy_Advance',
    'NES': 'Nintendo_-_Nintendo_Entertainment_System',
    'SNES': 'Nintendo_-_Super_Nintendo_Entertainment_System',
    'GG': 'Sega_-_Game_Gear',
    'SMS': 'Sega_-_Master_System_-_Mark_III',
    'MD': 'Sega_-_Mega_Drive_-_Genesis',
    'SEGACD': 'Sega_-_Mega-CD_-_Sega_CD',
    '32X': 'Sega_-_32X',
    'NGP': 'SNK_-_Neo_Geo_Pocket',
    'NGPC': 'SNK_-_Neo_Geo_Pocket_Color',
    'WS': 'Bandai_-_WonderSwan',
    'WSC': 'Bandai_-_WonderSwan_Color',
    'PS': 'Sony_-_PlayStation',
}
OUT = pathlib.Path(__file__).parent / 'card/Slot/System/Boxarts'
OUT.mkdir(parents=True, exist_ok=True)

for console, repo in REPOS.items():
    with tempfile.TemporaryDirectory() as tmp:
        url = f'https://github.com/libretro-thumbnails/{repo}.git'
        subprocess.run(['git', 'clone', '-q', '--filter=blob:none', '--no-checkout', '--depth', '1',
                        url, tmp], check=True)
        branch = subprocess.run(['git', '-C', tmp, 'rev-parse', '--abbrev-ref', 'HEAD'],
                                check=True, capture_output=True, text=True).stdout.strip()
        files = subprocess.run(['git', '-C', tmp, 'ls-tree', '-z', '--name-only', 'HEAD', 'Named_Boxarts/'],
                               check=True, capture_output=True, text=True).stdout.split('\0')
    names = sorted({
        f.removeprefix('Named_Boxarts/').removesuffix('.png')
        for f in files
        if f.endswith('.png') and not any(ord(c) < 32 for c in f)
    })
    header = f'# {repo}/{branch} Named_Boxarts from github.com/libretro-thumbnails (make-boxart-names.py)\n'
    (OUT / f'{console}.txt').write_text(header + '\n'.join(names) + '\n')
    print(f'{console}: {len(names)} covers', file=sys.stderr)
