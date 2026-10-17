#!/usr/bin/env python3
"""Regenerate card/Slot/System/<console>-cart-labels.txt from the LaunchBox Games Database.

The slot art set behind Cart Studio has no NES or SNES label scans, so Cart Art takes those
labels from LaunchBox's "Cart - Front" pictures: whole-cart photos it crops the label out of
(cart_art.rs). This keeps the ones of the console's standard US cart (PNG cut-outs, from the
regions that cart was sold in; Famicom, Super Famicom and PAL SNES carts are other shapes)
with the game's name and region, so the Brick never downloads the 100 MB metadata itself.
"famicom" keeps the NES games' Japanese pictures instead: the Famicom cart. The Sega, SNK and
Bandai consoles' lists work the same way ("mark3" is the Japanese Master System cart), and the
PlayStation's and Sega CD's list their "Disc" pictures, whose printing is the label.

The download is untrusted input: it is parsed with the standard library's expat (2.4+
guards against entity expansion), and only names without control characters and file names
of plain characters make it into the list, which the Brick checks again before any request.

Usage: python3 brick/make-cart-labels.py <console>|all [Metadata.zip]
"""
import pathlib, sys, tempfile, urllib.request, zipfile
import xml.etree.ElementTree as ET

URL = 'https://gamesdb.launchbox-app.com/Metadata.zip'
# LaunchBox regions as the No-Intro tags slot's name matching scores. The NES cart was the
# same in the US and Europe; the SNES cart of Europe (and Japan) is another shape.
AMERICAS = {'North America': 'USA', 'United States': 'USA', 'Canada': 'Canada', 'World': 'World', '': ''}
EUROPE = {'Europe': 'Europe', 'United Kingdom': 'UK', 'Germany': 'Germany', 'France': 'France',
          'Spain': 'Spain', 'Italy': 'Italy', 'Australia': 'Australia', 'Sweden': 'Sweden',
          'Netherlands': 'Netherlands'}
JAPAN = {'Japan': 'Japan'}
OTHERS = {'Brazil': 'Brazil', 'Asia': 'Asia', 'Korea': 'Korea', 'Oceania': 'Australia'}
EVERYWHERE = {**AMERICAS, **EUROPE, **JAPAN, **OTHERS}
WEST = {**AMERICAS, **EUROPE, **OTHERS}
CART, DISC, FANART = 'Cart - Front', 'Disc', 'Fanart - Cart - Front'
# console: (LaunchBox platform, regions whose pictures show that cart, picture type)
CONSOLES = {
    'nes': ('Nintendo Entertainment System', {**AMERICAS, **EUROPE}, CART),
    'famicom': ('Nintendo Entertainment System', JAPAN, CART),
    'snes': ('Super Nintendo Entertainment System', AMERICAS, CART),
    'md': ('Sega Genesis', EVERYWHERE, CART),
    'gg': ('Sega Game Gear', EVERYWHERE, CART),
    'sms': ('Sega Master System', WEST, CART),
    'mark3': ('Sega Master System', JAPAN, CART),
    '32x': ('Sega 32X', EVERYWHERE, CART),
    'ngp': ('SNK Neo Geo Pocket', EVERYWHERE, CART),
    'ngpc': ('SNK Neo Geo Pocket Color', EVERYWHERE, CART),
    'ws': ('WonderSwan', EVERYWHERE, CART),
    'wsc': ('WonderSwan Color', EVERYWHERE, CART),
    'segacd': ('Sega CD', EVERYWHERE, DISC),
    'ps': ('Sony Playstation', EVERYWHERE, DISC),
}
if len(sys.argv) < 2 or sys.argv[1] not in [*CONSOLES, 'all']:
    sys.exit(__doc__)
WANTED = list(CONSOLES) if sys.argv[1] == 'all' else [sys.argv[1]]
PLATFORMS = {CONSOLES[c][0] for c in WANTED}
SAFE = set('abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.')


def metadata():
    if len(sys.argv) > 2:
        return zipfile.ZipFile(sys.argv[2])
    tmp = tempfile.NamedTemporaryFile(suffix='.zip')
    request = urllib.request.Request(URL, headers={'User-Agent': 'slot-brick make-cart-labels'})
    with urllib.request.urlopen(request, timeout=600) as r:
        while chunk := r.read(1 << 20):
            tmp.write(chunk)
    tmp.flush()
    return zipfile.ZipFile(tmp.name)


games, names, carts = {}, {}, {}
with metadata().open('Metadata.xml') as f:
    for _, el in ET.iterparse(f):
        if el.tag == 'Game':
            if el.findtext('Platform') in PLATFORMS:
                games[el.findtext('DatabaseID')] = (el.findtext('Platform'), el.findtext('Name'))
            el.clear()
        elif el.tag == 'GameAlternateName':
            names.setdefault(el.findtext('DatabaseID'), []).append(
                (el.findtext('AlternateName') or '', el.findtext('Region') or ''))
            el.clear()
        elif el.tag == 'GameImage':
            if el.findtext('Type') in (CART, DISC, FANART):
                carts.setdefault(el.findtext('DatabaseID'), []).append(
                    (el.findtext('Type'), el.findtext('FileName') or '', el.findtext('Region') or ''))
            el.clear()

def lines_of(gid, name, regions, kind):
    lines = set()
    for file_kind, file, region in carts.get(gid, []):
        if file_kind != kind or region not in regions or not file.endswith('.png') \
                or not set(file) <= SAFE or len(file) > 64:
            continue
        tag = regions[region]
        for title, alt_region in [(name, '')] + names.get(gid, []):
            # The game's main (English) name too, so a ROM named for its US release finds
            # its Famicom picture.
            if (alt_region in regions or not alt_region) and title and not any(ord(c) < 32 for c in title):
                lines.add(f'{title} ({tag})\t{file}' if tag else f'{title}\t{file}')
    return lines


for console in WANTED:
    platform, regions, kind = CONSOLES[console]
    lines, fanart = set(), set()
    for gid, (game_platform, name) in games.items():
        if game_platform != platform:
            continue
        real = lines_of(gid, name, regions, kind)
        lines |= real
        # Fans' pictures of the cart, drawn from its real label, for a game with no photo:
        # listed after every photo, so a name with both tries the photos first.
        if not real and kind == CART:
            fanart |= lines_of(gid, name, regions, FANART)
    out = pathlib.Path(__file__).parent / f'card/Slot/System/{console}-cart-labels.txt'
    what = 'disc' if kind == DISC else 'cart'
    header = f'# {platform} {what} pictures from the LaunchBox Games Database ({URL}): name (region), file\n'
    out.write_text(header + '\n'.join(sorted(lines) + sorted(fanart)) + '\n')
    shown = sum(1 for g, (p, _) in games.items() if p == platform and any(k == kind for k, _, _ in carts.get(g, [])))
    print(f'{console}: {len(lines)} names for {shown} games, {len(fanart)} fan pictures; wrote {out}', file=sys.stderr)
