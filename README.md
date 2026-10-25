<img width="1920" height="1080" alt="brag" src="https://github.com/user-attachments/assets/fa574bb6-7bd4-4e78-8b72-14c58e3c716e" />

<div align="center">

# Azeroth Survivors

**A free fan-made WoW roguelike by [Aethus](https://wow.aethus.pl/), run locally on a
from-scratch WoW 1.12.1 client engine (Rust + [Bevy](https://bevy.org)).**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](LICENSE-MIT)

</div>

> **Not an original project.** *Azeroth Survivors* is a free, fan-made WoW roguelike by
> **Aethus** — play the original in the browser at **[wow.aethus.pl](https://wow.aethus.pl/)**
> ([r/wowservers announcement](https://www.reddit.com/r/wowservers/comments/1x0yidl/azeroth_survivors_roguelike_wow_singleplayer_game/)
> · [Discord](https://discord.gg/UzxaZuSAk)). This repository is a from-source local build of the
> game, running on the [benilla](#how-its-built) / [wenilla](#how-its-built) engine, for personal use.

Pick a class and a battleground, then survive: move as you would in WoW, your abilities cast themselves,
enemies swarm in waves, you level up and pick upgrades, and a boss shows up every couple of
minutes. **No server, no login, no account** — fully offline and self-contained.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/menu.jpg" alt="The pick screen: race, gender and class on the left, sixteen zones on the right, the hero in between"></td>
    <td width="50%"><img src="docs/screenshots/combat.jpg" alt="A Mage holding off Elwynn Forest's wolves with frost, fire and arcane"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/level-up.jpg" alt="Level up: three spell cards, coloured by rarity, each showing exactly what its next rank changes"></td>
    <td><img src="docs/screenshots/results.jpg" alt="The results screen after a death in Redridge Mountains"></td>
  </tr>
</table>

### In this build

- **A character-create pick screen** built from the client's own glue art: eight races, both
  genders, every class the race can be, and **sixteen zones**, from the starting zones to
  Naxxramas and the Gates of Ahn'Qiraj. Each zone fights with its own creatures and bosses, taken
  from the zone's real spawns. Your pick is remembered across reloads.
- **Level-up cards** that deal in, tilt toward the cursor and lift on hover. Their WoW item-quality
  colour says how strong a pick is, and each card lists exactly what changes, like
  `Damage 20 » 26` or `Targets 2 » 3`.
- **A class movement spell on F** from level 5, cast the way the hero faces: Blink, Charge,
  Sprint, Dash, Fade, Ghost Wolf, Aspect of the Cheetah, Blessing of Freedom, Howl of Terror.
- **World of Warcraft's own controls and camera** (1.12's model, rates included): W/S run and
  backpedal, A/D turn, Q/E strafe, right mouse steers, left mouse looks around, both buttons run,
  Num Lock autoruns, and the camera swings back behind you as you move.
- **The classic Escape menu:** Main Menu, Sound Options and Interface Options, drawn from the
  client's own frames, every setting saved.
- A light sharpen-and-colour pass over the picture (**G** toggles it).

## Desktop app and co-op (this fork)

This fork turns the game into a **native Windows desktop app** with **online co-op for up to four
players** through a dedicated server you host yourself (a $6 DigitalOcean droplet is plenty; see
[deploy/README.md](deploy/README.md)).

```bash
make fetch-data                 # the WoW 1.12.1 Data folder, as below
scripts/package-windows.sh      # → dist/AzerothSurvivors/Azeroth Survivors.exe (+ a Data link)
```

Or run it straight from the checkout: `WOW_DATA=wow-data/Data cargo run --release -p warcraft-survivors`.

**Co-op.** On the pick screen, set **Server** to your server's address once, then **Host Room**
and share the four-letter code (friends use **Join Code**), or everyone clicks **Quick Play**.
The room's leader presses **Enter World** to start for everyone, and friends can drop into a run
already under way.

- The party shares experience and levels together; each player picks their own cards (the
  field holds still while anyone is choosing, for at most 25 seconds).
- A fallen hero is not out: stand beside them for three seconds to revive them at half health.
  The run ends when the whole party is down.
- Healers' heals land on the most hurt teammate in range.
- The horde grows with the party (half again as many enemies per extra hero), bosses toughen
  for the whole party, and the experience curve scales with it.
- Teammates show on the minimap and in a party frame with their health, level and state.

**How it works.** The rules live in [`crates/survivors-sim`](crates/survivors-sim), plain Rust
with no engine. A solo run steps it inside the client; in co-op the server
([`crates/survivors-server`](crates/survivors-server)) steps it at 30 Hz and streams quantized
snapshots (≈4.6 KB at the thickest crowd) over UDP ([`crates/survivors-net`](crates/survivors-net),
renet + netcode). Each client moves its own hero over its own terrain and draws everyone else
between the two newest snapshots, so the field is smooth at any frame rate. The server needs no
WoW data. `survivors-bot` fills rooms with headless players for testing.

**Performance.** Native instead of WebAssembly: multithreaded rendering on Vulkan/DX12, no
browser in between, and a fat-LTO shipping build. On top of that the horde lives in a spatial
grid (separation, targeting, splash and areas touch only nearby cells), the view writes a unit's
transform only when it moved, and the minimap, which profiling showed recompositing its whole
image every frame (~1 ms), redraws at 20 Hz. Against the original's own native build, same
machine (RTX 5070), same late-game crowd (`WOW_SURVIVORS_AUTO=mage:late`): main thread median
**7.55 ms vs 8.07 ms**, and the slow frames (p90) **9.2 ms vs 11.5 ms**.

## Play it

```bash
git clone https://github.com/OutThisLife/azeroth-survivors-re
cd azeroth-survivors-re
make build     # fetch game data + install toolchain + build the wasm (~5.5 GB download)
make start     # → open http://127.0.0.1:8090/
```

`make build` chains three re-runnable scripts, each skipping what's already done:

| Step | What it does |
| --- | --- |
| `scripts/fetch-data.sh` | downloads the WoW 1.12.1 client and extracts its MPQs |
| `scripts/web-setup.sh` | installs the wasm toolchain (wasi-sdk, wasm-bindgen, binaryen) |
| `scripts/web-build.sh` | builds the client into `web/dist/` |

**Prerequisites:** [Rust](https://rustup.rs/), `make`, `curl`, `unzip`, and a bash shell. On Windows, run everything from Git Bash; PowerShell and cmd can't run the scripts.

Run `make` (or `make help`) to list every target.

## Controls

See [CONTROLS.md](CONTROLS.md). The short version: WoW's — **W/S** run and backpedal, **A/D**
turn, **Q/E** strafe, **right mouse** to steer, **left mouse** to look around, **Space** to jump,
**F** for your class's movement spell, the **mouse wheel** to zoom, **Esc** for the menu and its
options, **1/2/3** to pick a level-up card.

## The game data (not in this repo)

This repo ships **no Blizzard content** — no art, models, sounds, maps, MPQ contents, or
FrameXML. The game reads a WoW 1.12.1 client's `Data` folder at runtime, which you supply.

`make build` (via `scripts/fetch-data.sh`) downloads the **English WoW 1.12.1 (build 5875)**
client from [archive.org](https://archive.org/details/WoW_1.12.1-5875) (~5.5 GB) and extracts the
MPQs into `wow-data/Data/` (git-ignored, resumable, skipped when already present). Or point at a
client you already have with `--data` / `WOW_DATA`.

### Changing the download source

If the archive.org item moves or you have your own mirror, override the URL without editing
anything:

```bash
make build WOW_DATA_URL=https://your-mirror.example/client.zip
```

Or set it permanently by editing the single clearly-marked `DEFAULT_DATA_URL` line at the top of
`scripts/fetch-data.sh`. Either way the URL must be a direct link to a zip holding the client's
`Data/*.MPQ` files.

## How it's built

The survivors mode is `crates/benilla-app/src/survivors/`, running on two upstream projects:

- [benilla](https://github.com/samwhosung/benilla) — a from-scratch WoW 1.12.1 client.
- [wenilla](https://github.com/Arnesen/wenilla) — benilla compiled to WebAssembly, with a local
  host and realm service.

## Legal

*Azeroth Survivors* is a free fan game by **Aethus**, not affiliated with or endorsed by
Blizzard Entertainment, and it ships no Blizzard game files. The screenshots in `docs/screenshots/`
show World of Warcraft art rendered from a user-supplied client; that art is © Blizzard
Entertainment. World of Warcraft is a trademark of Blizzard Entertainment, Inc. The engine code in
this repository is licensed under
[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option; the vendored components under
`third_party/` keep their own upstream licenses.
