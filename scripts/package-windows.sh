#!/usr/bin/env bash
# Build the Windows desktop app and lay it out as a folder you can zip and hand to a friend:
#
#   dist/AzerothSurvivors/
#     Azeroth Survivors.exe      the game (player build, `ship` profile: fat LTO, no console)
#     Data/                      the WoW 1.12.1 Data folder: a link to wow-data/Data here;
#                                a friend drops their own client's Data folder in its place
#     README.txt
#
# Re-runnable. Run from Git Bash: `scripts/package-windows.sh`. `PROFILE=release` builds faster
# with slightly slower code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${PROFILE:-ship}"
OUT="$ROOT/dist/AzerothSurvivors"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"

echo "building warcraft-survivors (profile $PROFILE)…"
cargo build --profile "$PROFILE" -p warcraft-survivors --no-default-features --manifest-path "$ROOT/Cargo.toml"

mkdir -p "$OUT"
cp "$TARGET/$PROFILE/warcraft-survivors.exe" "$OUT/Azeroth Survivors.exe"

if [ ! -e "$OUT/Data" ] && [ -d "$ROOT/wow-data/Data" ]; then
    # A directory junction: no copy of the 5 GB, and no admin rights needed.
    cmd //c mklink //J "$(cygpath -w "$OUT/Data")" "$(cygpath -w "$ROOT/wow-data/Data")" > /dev/null
fi

cat > "$OUT/README.txt" <<'EOF'
Azeroth Survivors — desktop edition
===================================

1. Put a World of Warcraft 1.12.1 (build 5875) client's "Data" folder next to this file, so
   the MPQs are at Data\patch.MPQ, Data\terrain.MPQ and so on. (Or set the WOW_DATA environment
   variable to that folder.)
2. Run "Azeroth Survivors.exe".

Co-op (up to 4 players)
-----------------------
On the pick screen, under "Co-op":
  * Server     — type your group's server address (IP or host name, port 7777 by default).
  * Name       — the name your party sees.
  * Host Room  — opens a room and shows a four-letter code; give it to your friends.
  * Join Code  — type a friend's code.
  * Quick Play — joins any open public room, or opens one.
In a room, the leader's "Enter World" starts the run for everyone. Friends can also join a run
that is already under way.

The party shares experience and levels together; everyone picks their own cards. A fallen hero
is not out: stand beside them for three seconds to bring them back. The run ends when the whole
party is down.

Controls: WASD move · Space jump · F movement spell (level 5) · mouse wheel zoom · C free camera
· Esc pause (solo) / menu (co-op) · 1/2/3 pick a card.
EOF

echo "ready: $OUT"
