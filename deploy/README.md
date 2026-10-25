# Hosting a co-op server

Azeroth Survivors' co-op runs on a dedicated server: `survivors-server`, a small Rust program
that hosts rooms of up to four players and runs every room's game. It reads **no game data**
(no WoW client files go on the server), so a cheap Linux box is enough. Each player's own
desktop client draws the world from their own WoW 1.12.1 data.

| | |
|---|---|
| Protocol | UDP (renet + netcode), port **7777** |
| Tick | 30 Hz simulation, 15 Hz snapshots |
| CPU | ~0.3 ms per room per tick at the end-game crowd: one vCPU hosts dozens of rooms |
| Bandwidth | ~25–70 KB/s down per player at the thickest crowd, a few KB/s up |
| Memory | a few MB per room |

## DigitalOcean, one paste

1. **Create → Droplets.** Ubuntu 24.04, Basic, Regular, **$6/mo (1 GB / 1 vCPU)** is plenty.
   Pick the region closest to your group.
2. Under **Advanced options → Add Initialized scripts (free)**, paste the whole of
   [`digitalocean/cloud-init.yml`](digitalocean/cloud-init.yml).
3. Add your SSH key and create it. On first boot it installs Docker, opens UDP 7777 in the
   firewall, builds the server from this repository and starts it. That takes about five
   minutes.
4. In the game: on the pick screen click **Server**, type the droplet's IP (e.g.
   `203.0.113.7`, the port defaults to 7777), press Enter. Then **Host Room** and read your
   room code out to your friends, who click **Join Code**; or everyone clicks **Quick Play**.

If you also use a **DigitalOcean Cloud Firewall**, add an inbound rule for **UDP 7777**.

With the `doctl` CLI instead of the web console:

```bash
doctl compute droplet create azeroth-survivors --image ubuntu-24-04-x64 --size s-1vcpu-1gb --region nyc3 --ssh-keys <your-key-id> --user-data-file deploy/digitalocean/cloud-init.yml --wait
```

## Running it

```bash
ssh root@<droplet-ip>
docker logs -f survivors        # who connects, rooms opening and closing, a status line a minute
survivors-update                # pull the latest server from GitHub, rebuild, restart
```

`survivors-update` builds the `coop-desktop` branch of `DevonThomas/azeroth-survivors-re`; set
`REPO=` / `BRANCH=` in front of it to build another. Clients and server must speak the same
protocol (`PROTOCOL_ID` in `crates/survivors-net/src/lib.rs`): update the server whenever the
players update their client, and the other way round.

## Without Docker

Any Linux or Windows machine with Rust:

```bash
cargo build --release -p survivors-server
./target/release/survivors-server --bind 0.0.0.0:7777
```

Options (also settable as environment variables): `--bind` / `AZS_BIND`, `--max-clients` /
`AZS_MAX_CLIENTS` (default 256), `--status-secs` / `AZS_STATUS_SECS` (default 60).

## Testing a server

`survivors-bot` connects headless players. On any machine with the repository:

```bash
cargo run --release -p survivors-server --bin survivors-bot -- --server <ip>:7777 --bots 3 --secs 60
cargo run --release -p survivors-server --bin survivors-bot -- --server <ip>:7777 --bots 2 --join ABCD
```

The first fills a room with three bots and starts it; the second sends two bots into room
`ABCD`, so you can play with company. Each bot prints its round trip, the snapshots it got and
the party's level.

## Security notes

The server runs netcode in **unsecure** mode: anyone who has the address and the client can
connect, and names are self-chosen. That is the usual setup for a server among friends. For a
public service, issue netcode *connect tokens* from a small login backend
(`renet_netcode::ServerAuthentication::Secure`) so only signed-in players get in. The server
validates what clients send (positions are held to the battleground, picks and casts are
checked against the hero's own state), and a client never decides damage, kills or loot.
