# Slot. for TrimUI Brick — v0.23

An unofficial port of **Slot.**, Brandon T. Kowalski's GB/GBC/GBA frontend for the Anbernic RG SP,
to the **original TrimUI Brick** on stock firmware, with NES and SNES added. Port by **LordNordyx**.

> **Note:** Report problems at <https://github.com/LordNordyx/slot-brick>, not to the upstream project.

This is a complete SD card install. You don't need any earlier release, and copying the card
doesn't flash anything to the device.

---

## Installation

1. **Back up.** Power off the Brick and copy your entire current SD card to your PC.
2. **Prepare a card.** Use an empty SD card; a spare is safest. If you reuse your current card,
   verify the backup first, then delete everything on it (no reformat needed).
3. **Copy Slot.** Copy everything **inside** the `SD-CARD` folder to the root of the card,
   including the hidden `.slot-boot` folder. Don't copy the `SD-CARD` folder itself.
   On a Mac, Finder hides that folder: press `Cmd + Shift + .` to show it before selecting.
   If it gets left behind anyway, opening **Slot** once from the stock Apps menu puts it back.
4. **Restore your files** from the backup into the matching folders:
   - `Slot/Games`, `Slot/Saves`, `Slot/States`, `Slot/BIOS`
   - `Slot/Config`, `Slot/Labels`, `Slot/Wallpapers`
   - Any `Slot/BootLogo/original*` files

   ⚠️ Do **not** restore old `Slot/System`, `trimui`, `Tools` or `Apps` folders.
5. **Start.** Insert the card and power on. If the stock menu appears instead of Slot, open
   **Apps → Slot** once to set up automatic startup.

---

## Controls

`HOME` is the left small button below the screen (labelled Menu) and `SELECT` is the middle one.
`A + B` means hold A and press B; `A / B` means either button.

### On the carousel

| Button           | Action                                  |
| ---------------- | --------------------------------------- |
| `Left` / `Right` | Browse games                            |
| `L1` / `R1`      | Switch shelves (one per console)  |
| Tap `A`          | Resume from the last save state         |
| Hold `A`         | Start the game fresh                    |
| `START`          | Cart colour and finish (GBA: and core)   |
| `HOME`           | Menu (settings, Wi-Fi, About)           |

### In game

| Button                      | Action                                              |
| --------------------------- | --------------------------------------------------- |
| Hold `HOME`                 | Save, eject and return to the carousel              |
| Double-tap `HOME`           | Save state switcher                                 |
| `HOME + B`                  | LEDs on / off                                       |
| `SELECT + HOME`             | Game menu (multiplayer Host / Join)                 |
| `SELECT + R1`               | Save state                                          |
| `SELECT + L1`               | Load the latest save state                          |
| `SELECT + A` / `B`          | Next / previous shader                              |
| `SELECT + X`                | GBA/SNES/MD/WS: original or full screen (per game)  |
| `SELECT + Y`                | Colour correction on / off (GB, GBC, GBA)           |
| `SELECT + L2` / `R2`        | GB: previous / next palette; CD games: change disc  |
| `SELECT + Up` / `Down`      | Brightness                                          |
| `SELECT + Left` / `Right`   | Blue light filter                                   |
| `L1` / `R1`                 | GB/GBC/NES/GG/SMS/NGP: stretched / original picture |
| Hold `L2`                   | Rewind                                              |
| Hold `R2`                   | Fast-forward (double-tap to lock it on)             |

### System

| Button             | Action                                 |
| ------------------ | -------------------------------------- |
| Volume `+` / `−`   | Volume (press both together to mute)   |
| `Fn` slider        | Mute / unmute                          |
| Press `Power`      | Sleep / wake                           |
| Hold `Power`       | Shut down, or exit to the stock OS     |

**Cart colour:** press `START` on a game on the shelf to change its cartridge: any colour and a
solid, see-through or glitter finish for Game Boy and GBA carts, and the shape: original or Color
for Game Boy carts, NES or Famicom for NES carts, US or Europe / Japan for SNES carts. NES and
SNES carts are always their system's grey, like the real ones; a NES game on the Famicom cart
can have any colour, as Famicom carts came in many. On a GBA cart `START` opens the core picker: `Left` / `Right` swap the core, `Up` changes the colour
and `Down` the finish. The cart changes as you go; `A` saves, `B` puts it back. Choices are kept
in `Slot/Labels/cart_shell.ini`.

**NES:** put `.nes` ROMs in `Slot/Games/NES`; they get their own shelf of NES cartridges and run
on FCEUmm. No BIOS is needed. NES carts are grey; a ROM named as a Japanese release, such as
`Rockman (Japan).nes`, or on a chip only Famicom carts used, gets the small, landscape Famicom
cart instead, in a colour picked from its name (change it under `START` → Colour). NES ROMs don't say where they were
sold, so for other Japanese games choose Famicom under `START` → Shape. Cart Art fetches NES labels too (see below), or put your own picture in
`Slot/Labels/NES`. The picture is the TV's 256 x 224, padded to 4:3 at actual size or
stretched to fill the screen with `L1` / `R1`. RetroAchievements work for NES too.

**SNES:** put `.sfc` or `.smc` ROMs in `Slot/Games/SNES`; they get a shelf of SNES cartridges and
run on Snes9x. No BIOS is needed. European and Japanese games get the rounder PAL / Super Famicom
cart; Slot reads where a game was sold from the ROM, so a ripped `Terranigma.sfc` works too. `L1` / `R1` and `X` / `Y` are the SNES's own buttons (no turbo
on SNES), so `SELECT + X` switches the picture between original and full screen. Cart Art fetches
SNES labels too, and RetroAchievements work.

**Game Gear and Master System:** put `.gg` ROMs in `Slot/Games/GG` and `.sms` ROMs in
`Slot/Games/SMS`; each gets a shelf of black Sega cartridges and runs on PicoDrive. No BIOS is
needed. Button 1 is `B`, button 2 is `A`, and `START` is Start (Pause on the Master System).
`L1` / `R1` switch between the original and stretched picture. RetroAchievements work for both.
Cart Art has no labels for them yet, so put your own pictures in `Slot/Labels/GG` and
`Slot/Labels/SMS` if you want them. PicoDrive is free for non-commercial use only.

**Mega Drive:** put `.md`, `.gen`, `.smd` or `.bin` ROMs in `Slot/Games/MD`; they get a shelf of
black Mega Drive cartridges and run on PicoDrive. No BIOS is needed. The pad is the six-button
one: `Y`, `B`, `A` are A, B, C, and `X`, `L1`, `R1` are Y, X, Z. As on the SNES, `SELECT + X`
switches between the original and full-screen picture. RetroAchievements work; labels go in
`Slot/Labels/MD`.

**Neo Geo Pocket and Color:** put `.ngp` ROMs in `Slot/Games/NGP` and `.ngc` ROMs in
`Slot/Games/NGPC` (either extension works in either folder); they run on Beetle NeoPop, set to
English. No BIOS is needed. `B` and `A` are the console's A and B, `START` is Option, and
`L1` / `R1` switch between the original and stretched picture. RetroAchievements work.

**Sega CD:** put `.cue` (with its `.bin` tracks), `.chd`, `.iso` or `.m3u` images in
`Slot/Games/SEGACD`, loose or each game in a folder of its own. A game of several discs plays
from an `.m3u` listing them, and `SELECT + L2` / `R2` changes disc. It needs the BIOS of the
game's region in `Slot/BIOS`: `bios_CD_U.bin` (US), `bios_CD_E.bin` (Europe) or `bios_CD_J.bin`
(Japan); without one Slot says which file is missing. Controls are the Mega Drive's.
RetroAchievements work for `.cue` and `.iso` images, not yet `.chd`.

**Sega 32X:** put `.32x` ROMs in `Slot/Games/32X`. No BIOS is needed and the controls are the
Mega Drive's. Most games run at full speed; a few of the heaviest (Virtua Racing Deluxe) may not.

**WonderSwan and Color:** put `.ws` ROMs in `Slot/Games/WS` and `.wsc` ROMs in `Slot/Games/WSC`;
they run on Beetle Cygne. No BIOS is needed. Held flat, the D-pad is the X pad, `A` / `B` are A
and B and `L1` / `R1` the Y pad's left and right. A game made to be played upright (Gunpey,
Judgement Silversword...) is shown upright: then the D-pad is the Y pad, the four face buttons
the X pad, and `L1` / `R1` are A and B. `SELECT + X` switches the picture. RetroAchievements work.

**Cart art:** connect to Wi-Fi, then open **HOME → Cart Art**.
**GET CART ART** gives every game with no label a scan of its real cartridge label; labels you
already have are never replaced. The scans come from the slot art set behind
[Cart Studio](https://studio.slot-cfw.fyi) (scans from ScreenScraper), downloaded over HTTPS.
A game is found by the ROM's checksum, or for a trimmed or patched ROM by its file name
(helped by the GBA game code in its header). ROM hacks and homebrew keep the label with their
file name, so they never look like the game they are based on. Each ROM is read once and its
checksum kept in `Slot/System/Cache/crc32.txt`.

NES and SNES labels come from the [LaunchBox Games Database](https://gamesdb.launchbox-app.com)'s
pictures of the cartridges: Slot finds the game by its ROM's file name (No-Intro names work best,
for example `Mega Man 2 (USA).nes`) and cuts the label out of a picture of the standard US cart.
Games on the Famicom cart get a Famicom cart's label (from the Japanese release), kept apart in
`Slot/Labels/NES/Famicom` so a NES game can have both: swapping its shape swaps the label. While
a game has only one, both carts show it. Carts of another shape (Tengen, gold Zelda, PAL SNES, unlicensed)
keep their file-name label, as do games LaunchBox has no cart picture of (many SNES games).

`Slot/Logs/cart-art.txt` lists each game and what it got. Below GET CART ART, every game with a
label is listed: press A twice on one to delete its label, then GET CART ART fetches a new one.

**Auto-sleep:** the Brick sleeps after 5 minutes idle in menus or 15 minutes in a game.
To turn this off, create an empty file named `Slot/DISABLE_AUTO_SLEEP`.

---

## Apps

These appear in the stock OS under **Apps**.

| App       | What it does                                                         |
| --------- | -------------------------------------------------------------------- |
| **Slot**  | Starts Slot and sets it to start automatically                       |
| **Logo**  | Sets your own boot logo, or restores the original (see below)        |
| **Check** | Reports why Slot did or did not start, in `startup-report.txt`       |
| **Stock** | Turns off Slot autostart and restores the stock startup              |

### Custom boot logo

1. Put one image (`.png`, `.jpg`, `.jpeg` or `.bmp`) in `Apps/BootLogo/Image`.
2. Open **Apps → Logo**, then restart.

To restore the original, empty the `Image` folder and open **Logo** again.

> **Keep a backup:** the first run saves the original logo to `Slot/BootLogo/original.bmp`.
> Copy that file to your PC; it's the only way back.

---

## Multiplayer (experimental)

1. Install this same version on both Bricks, with the same game, core and BIOS.
2. In game, press `SELECT + HOME`.
3. Choose **Host** on one Brick first, then **Join** on the other.

No router is needed. This hasn't been tested on two physical devices yet. If it fails, send
`Slot/Logs/multiplayer.log` and `Slot/Logs/slot.log`.

---

## RetroAchievements (experimental)

1. Connect the Brick to Wi-Fi.
2. Press **HOME** and open **RetroAchievements**. Enter your
   [retroachievements.org](https://retroachievements.org) username and password with the
   on-screen keyboard (START when done), then choose **Log in**.
3. Start a GBA, GB or GBC game. A small pop-up in the top-right corner shows the game's
   achievement count and each one you unlock, with its badge.

Only a login token is kept, in `Slot/Config/retroachievements.txt`; your password is never
saved. The same screen can test the connection, show a test pop-up, and log out. If the game
starts before Wi-Fi has an address, Slot signs in again once it does.

The `retroachievements.ini` file used by earlier builds is no longer read. Delete it, since it
holds your password in plain text.

Achievements are **softcore only** for now: save states, rewind and fast forward stay
available, and unlocks count toward softcore. If nothing appears, check `Slot/Logs/slot.log`
for lines starting with `slot: cheevos`.

---

## Cloud sync (experimental)

Keeps `Slot/Saves` and `Slot/States` in step with a folder on any WebDAV server: Nextcloud,
ownCloud, Koofr, a Synology or other NAS, or `rclone serve webdav` on a computer.

1. Connect the Brick to Wi-Fi.
2. Create `Slot/Config/cloudsync.ini`:

   ```ini
   url=https://cloud.example.com/remote.php/dav/files/YourName/Slot
   username=YourName
   password=an-app-password
   ```

   On Nextcloud, make an app password under *Settings → Security* rather than using your
   real one. The folder is created if it doesn't exist.

Sync runs in the background when Slot starts, as soon as you close a game, and every 10
minutes. A banner says when files were sent or received. Use the same folder on every device
to carry a game between them.

- While a game is running, its own save and states are only ever **sent**. Anything newer
  for it on the server comes down after you close the game.
- If a file changed on both sides, the newer copy wins. The other is kept on the card next to
  it as `<name>.conflict-<date>`, so nothing is lost.
- Deleting a state deletes it on the other side too. Saves are never deleted by sync: a save
  missing on one side is copied back from the other.
- With only one game on the card, Slot never leaves it, so saves from other devices only
  arrive after you close it.

Add `enabled=0` to pause sync. Details are in `Slot/Logs/slot.log` on lines starting with
`slot: cloud`.

---

## Troubleshooting

| Problem                               | Fix                                                                 |
| ------------------------------------- | ------------------------------------------------------------------- |
| Stock menu appears instead of Slot    | Open **Apps → Check** and read `startup-report.txt` on the card     |
| You want to skip Slot at startup      | Create an empty file `Slot/DISABLE_AUTOBOOT`; delete it to re-enable |
| You need stock emulators or RetroArch | They aren't included; keep your original card backup                |
