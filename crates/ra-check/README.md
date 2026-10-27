# ra-check

Checks a folder of games against RetroAchievements, from a Mac (or Linux). For every game it
says whether RetroAchievements supports that exact file, and if not, whether it has the game
in another version, listing the file names that work.

Each file is hashed with the same rcheevos code slot runs on the handheld (including slot's
CHD reader), so a game marked supported here is one slot will find achievements for.

## Using it

    ./ra-check /Volumes/trimui/Games

The first run asks for your RetroAchievements **web API key** (retroachievements.org/settings,
under "Keys") and keeps it in `~/Library/Application Support/ra-check/api-key`. Point it at a
Games folder holding `GBA`, `SNES`, `PS`... folders, at one system's folder, or at single files.
Tip: type `./ra-check ` in Terminal, then drag the folder onto the window.

    ✗ Metroid Fusion (Germany).gba  different version needed for Metroid Fusion (41 achievements)
        works: Metroid Fusion (USA).gba
    ✓ Pokemon - Emerald Version (USA, Europe).gba  Pokémon Emerald Version · 54 achievements

| Mark | Meaning |
| --- | --- |
| ✓ | RetroAchievements supports this file. |
| ✗ | It has the game, but not this version. The "works:" lines are the files it takes (some only after applying its patch). |
| · | RetroAchievements knows the game but has no achievements for it yet. |
| ? | Neither the file nor its title is on RetroAchievements (homebrew, hacks, a renamed file). |
| ! | The file couldn't be read as a game for that system. |

The "different version" match goes by title, the way No-Intro and Redump name files, so a
renamed ROM shows as `?` rather than `✗`.

Options: `--only-problems`, `--csv results.csv` (for Numbers or Excel), `--system GBA` (when the
folder name doesn't say), `--refresh` (lists are cached for 3 days in
`~/Library/Caches/ra-check`), `--hashes` (just the hashes, offline), `--forget-key`.

Systems: GBA, GB, GBC, NES (and FDS), SNES, MD, GG, SMS, NGP/NGPC, WS/WSC, SEGACD, 32X, PS, N64
and DS. Discs can be `.chd`, `.cue` or `.iso`; cartridges can also be zipped.

## Building

    cargo build --release -p ra-check

The `ra-check` workflow builds a universal macOS binary on every change. From Linux,
`cargo zigbuild --release -p ra-check --target universal2-apple-darwin` makes one too.
