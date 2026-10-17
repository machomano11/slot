# Changelog — Slot. for TrimUI Brick

## Unreleased
- Cart Art covers every console: a game with no label scan or cart picture (all Game Gear,
  Master System, Mega Drive, 32X, Sega CD, Neo Geo Pocket, WonderSwan and PlayStation games, and
  the many NES and SNES games LaunchBox has no cart of) gets its front cover as a label, from
  libretro's thumbnails on GitHub, found by the ROM's file name.
- RetroAchievements now recognise PlayStation and Sega CD games kept as `.chd`, read the same
  way as their `.cue`/`.bin`, so they unlock achievements instead of failing to identify.
- PlayStation: `.cue`, `.chd`, `.pbp`, `.iso` and `.m3u` images in `Slot/Games/PS` get a shelf of
  discs and run on PCSX ReARMed with its ARM64 recompiler, save states and
  RetroAchievements. The BIOS is optional (`scph5501.bin` and friends in `Slot/BIOS`; a built-in
  one is used otherwise). `L2` / `R2` go to the game, so fast-forward moves to `HOME + R2` and
  PlayStation games have no rewind. `SELECT + L2` / `R2` changes disc.
- Sega 32X: `.32x` ROMs in `Slot/Games/32X` get a shelf of black 32X cartridges and run on
  PicoDrive with its SH2 recompiler, the Mega Drive's six-button pad, save states, rewind,
  fast-forward and RetroAchievements. A few heavy games may not hold full speed.
- Sega CD (Mega-CD): `.cue`, `.chd`, `.iso` and `.m3u` images in `Slot/Games/SEGACD` (a game may
  sit in a folder of its own) get a shelf of silver discs and run on PicoDrive, with the
  Mega Drive's six-button pad, save states, rewind, fast-forward and RetroAchievements (for
  `.cue`/`.iso`). It needs a BIOS in `Slot/BIOS`: `bios_CD_U.bin`, `bios_CD_E.bin` or
  `bios_CD_J.bin` for US, European and Japanese games.
- Groundwork for disc systems: a core that reads its own files is handed only the image's path
  (no more loading a whole CD into memory), games of several discs swap with `SELECT + L2` / `R2`
  through their `.m3u`, a disc game can sit in a folder of its own, a console that needs a BIOS
  says which file to put in `Slot/BIOS` instead of starting to a black screen, and
  RetroAchievements can identify CD images.
- WonderSwan and WonderSwan Color: `.ws` and `.wsc` ROMs in `Slot/Games/WS` and `Slot/Games/WSC`
  get shelves of near-black Bandai cartridges and run on Beetle Cygne, with save states, rewind,
  fast-forward and RetroAchievements. A game made to be played upright is shown upright (read
  from its header), with the controls turned to match. `SELECT + X` switches the picture.
- Neo Geo Pocket and Neo Geo Pocket Color: `.ngp`, `.ngc` and `.ngpc` ROMs in `Slot/Games/NGP` and
  `Slot/Games/NGPC` get shelves of slate-grey SNK cartridges and run on Beetle NeoPop (in
  English), with save states, rewind, fast-forward and RetroAchievements. `B` and `A` are the
  console's A and B, `START` is Option, and L1/R1 switch the picture, as on the Game Gear.
- Mega Drive: `.md`, `.gen`, `.smd` and `.bin` ROMs in `Slot/Games/MD` get a shelf of black
  Mega Drive cartridges and run on PicoDrive with a six-button pad, save states, rewind,
  fast-forward and RetroAchievements. `Y`, `B`, `A` are the pad's A, B, C and `X`, `L1`, `R1`
  its Y, X, Z, so `SELECT + X` switches between the original and full-screen picture.
- Game Gear and Master System: `.gg` ROMs in `Slot/Games/GG` and `.sms` ROMs in
  `Slot/Games/SMS` get their own shelves of black Sega cartridges and run on PicoDrive, with
  save states, rewind, fast-forward and RetroAchievements. L1/R1 switch the picture, as on the
  NES. The Game Gear has the LCD shaders, the Master System Off and Grid. Their carts have one
  shape and colour, so START on them does nothing, and Cart Art has no labels for them yet.
- New HOME > Cart Art: GET CART ART gives games with no label a real cartridge label scan (by
  ROM checksum, from the slot art set behind Cart Studio; NES and SNES labels from LaunchBox cart
  pictures), over HTTPS. Existing labels are never replaced, and labels can be deleted there one
  by one. From Slot-Brick.
- RetroAchievements now uses Slot-Brick's version: log in from HOME > RetroAchievements
  instead of editing a file, and unlocks show as corner pop-ups with their badges.
- A game started before Wi-Fi had an address signs in to RetroAchievements again once it does.
- Cart colours: START on a game changes its cartridge's colour, finish (solid, see-through,
  glitter) and, for Game Boy carts, its shape, previewed live. NES and SNES carts only change
  shape (NES or Famicom; US or PAL), as the real ones came in their system's grey; a Famicom
  cart can have any colour. On GBA carts START opens the core
  picker, where Up/Down change the colour and finish.
- SNES: European and Japanese games get the rounder PAL / Super Famicom cart, read from the ROM's
  header so ripped ROMs with no region in their name work too.
- NES: Famicom games get the real Famicom cart, small, landscape and in colour, instead of a
  coloured NES cart. Cart Art fetches Famicom labels for them from LaunchBox's pictures of
  Japanese carts, kept in `Labels/NES/Famicom` beside the NES label, so swapping a game between
  the NES and Famicom cart swaps its label too. A tall NES label on a Famicom cart is shown whole
  instead of cut down.
- NES: a ROM on a chip only Famicom carts used (Konami VRC, Namco 163 and others) gets a Famicom
  plastic even without "(Japan)" in its name.
- The core picker shows the cart in its real plastic instead of the default black, and its
  legend fits the Brick's screen.
- SNES: `.sfc`/`.smc` ROMs in `Slot/Games/SNES` get a shelf of SNES cartridges and run on Snes9x,
  with save states, rewind, fast-forward and RetroAchievements. X, Y, L and R are the game's own;
  SELECT + X switches the picture. Cart Art fetches SNES labels from LaunchBox cart pictures.
- Slot creates the NES and SNES folders on the card itself at startup.
- NES: `.nes` ROMs in `Slot/Games/NES` get a shelf of NES cartridges (grey; Famicom colours for
  Japanese releases) and run on FCEUmm (GPL-2.0), with save states, rewind, fast-forward and
  RetroAchievements. The emulator picture canvas is now sized per system (256x240 for NES); the
  Game Boys and GBA are unchanged.
- The NES cart is drawn after the real one: a full-height grip on the left, the label recess
  open to the top on the right, the arrow below it.

## v0.23
- Settings > About credits the port to LordNordyx.
- Short app names in the stock Apps list: Slot, Logo, Check and Stock, each with its own icon.
  Folder names are unchanged, so existing Apps/BootLogo/Image pictures keep working.
- Card README rewritten with the Brick's actual button layout (HOME = left button, SELECT = middle).
- Removed leftovers: the old Tools/tg5040/Slot.pak launcher and the RetroAchievements research note.

## v0.22.2
- Fixed "works once, then stock menu": the startup hook now waits until the real SD card is
  mounted instead of reading a stray Slot folder on internal storage.
- Check lists files hidden under the card mount on internal storage.

## v0.22.1
- The startup hook waits up to 30 s (was 10 s) for slow-mounting cards and logs each decision
  to Slot/Logs/hook.log.

## v0.22
- A read-only or damaged card no longer drops silently to the stock menu; Slot shows
  "Can't write to SD card".
- Check writes startup-report.txt at the top of the card.

## v0.21
- Fixed autostart failing after a firmware update or reinstall.
- New Check app: read-only startup report.

## v0.17 – v0.20
- SELECT + X switches GBA between 3:2 and 4:3 full screen, remembered per game.
- About screen describes the Brick port.

## v0.16
- Waking no longer needs two Power presses; Wi-Fi reconnects in the background.

## v0.15
- Merged upstream Slot 1.6.0: GB cartridge clocks kept in saves, read-only card alarm, faster
  library loading, shuffled wallpapers, crash logs.
- Boot Logo app (now Logo) replaces the separate install/restore logo apps.

## v0.14
- Lower sleep drain: radios off during suspend, real suspend-to-RAM where available.

## v0.13
- Auto-sleep after 5 min idle in menus or 15 min in game (Slot/DISABLE_AUTO_SLEEP turns it off).

## v0.12
- Wi-Fi works on stock firmware; minimum CPU clock in games fixes crackling audio.

Earlier versions: Brick screen, input, LEDs, power menu and autostart (see SOURCES.md).
