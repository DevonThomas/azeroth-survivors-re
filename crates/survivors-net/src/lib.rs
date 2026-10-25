//! The co-op wire protocol: what a client and the dedicated server say to each other over renet
//! (UDP, netcode), and [`Client`], the client's end of the connection.
//!
//! Two channels each way. Channel 0 is unreliable: the client's hero position, the server's
//! snapshots and visual events, all superseded by the next. Channel 1 is reliable and ordered:
//! lobby traffic, card picks, and the events that must land (teleports, banners, revives).

#[cfg(feature = "transport")]
mod client;
mod hero;

use serde::{Deserialize, Serialize};
use survivors_sim::{Event, Id, Snapshot, Vec2, Vec3};

#[cfg(feature = "transport")]
pub use client::{Client, ClientError, Status};
pub use hero::HeroState;

/// Bumped on every incompatible change; a mismatch is refused at the handshake.
pub const PROTOCOL_ID: u64 = 0x415A_5355_0000_0006;
pub const DEFAULT_PORT: u16 = 7777;
/// The server steps every room this often…
pub const TICK_HZ: u32 = 30;
/// …and sends a snapshot every this many ticks.
pub const SNAPSHOT_EVERY: u32 = 2;
/// Players per room.
pub const ROOM_SIZE: usize = survivors_sim::MAX_PLAYERS;

pub const UNRELIABLE: u8 = 0;
pub const RELIABLE: u8 = 1;

#[cfg(feature = "transport")]
pub fn connection_config() -> renet::ConnectionConfig {
    use renet::{ChannelConfig, ConnectionConfig, SendType};
    use std::time::Duration;
    let channels = || {
        vec![
            ChannelConfig {
                channel_id: UNRELIABLE,
                max_memory_usage_bytes: 2 * 1024 * 1024,
                send_type: SendType::Unreliable,
            },
            ChannelConfig {
                channel_id: RELIABLE,
                max_memory_usage_bytes: 4 * 1024 * 1024,
                send_type: SendType::ReliableOrdered {
                    resend_time: Duration::from_millis(150),
                },
            },
        ]
    };
    ConnectionConfig {
        available_bytes_per_tick: 120_000,
        server_channels_config: channels(),
        client_channels_config: channels(),
    }
}

/// The hero a player brings: the class (an index into `data::CLASSES`) and the look picked.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    pub name: String,
    pub class: u8,
    pub race: u8,
    pub sex: u8,
    pub display: u32,
    /// What the hero brings from the player's account: gear, talents, perks, look. The server
    /// clamps it before the run sees it.
    pub loadout: survivors_sim::Loadout,
}

/// Where a client's hero is, sent every frame or so on the unreliable channel.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct InputMsg {
    pub pos: Vec3,
    pub yaw: f32,
    pub flags: u32,
    pub speed: f32,
    pub vy: f32,
    pub seq: u32,
}

impl From<InputMsg> for survivors_sim::HeroInput {
    fn from(m: InputMsg) -> Self {
        Self {
            pos: m.pos,
            yaw: m.yaw,
            flags: m.flags,
            speed: m.speed,
            vy: m.vy,
            seq: m.seq,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Open a new room and wait in its lobby; `public` rooms are found by Quick Play.
    Create { pick: Pick, map: u16, public: bool },
    /// Into the room with this code, in its lobby or straight into its run.
    Join { code: String, pick: Pick },
    /// Into any public room with space, or a new public one.
    Quick { pick: Pick, map: u16 },
    /// A new hero while still in the lobby.
    SetPick(Pick),
    /// The room's leader changes the battleground.
    SetMap(u16),
    /// The room's leader starts the run.
    Start,
    Leave,
    Input(InputMsg),
    Mobility { aim: Option<Vec2> },
    Choose { index: u8 },
    /// Deal a fresh hand (a reroll), banish card `index`, or pass on the hand for gold.
    Reroll,
    Banish { index: u8 },
    Skip,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub client: u64,
    pub pick: Pick,
    /// The hero's id in the running sim, while there is one.
    pub hero: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lobby {
    pub code: String,
    pub leader: u64,
    pub map: u16,
    pub public: bool,
    pub members: Vec<Member>,
    /// A run is under way: joining drops straight in.
    pub running: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    /// The room this client is in, on every change.
    Lobby(Lobby),
    /// Out of the room (left, or it closed).
    NoRoom,
    /// The run began (or this client dropped into one): `you` is this client's hero.
    Started { map: u16, you: Id },
    Snapshot(Box<Snapshot>),
    /// This client's own hero, for its HUD and cards.
    Hero(Box<HeroState>),
    Events(Vec<Event>),
    /// The run is over; the last snapshot has the result.
    Ended,
    Error(String),
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_stdvec(msg).expect("survivors-net: encode")
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

/// Room codes: four letters, no vowels-that-read-as-digits.
pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

pub fn normalize_code(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A player name for the netcode user data: at most 32 bytes of UTF-8.
#[cfg(feature = "transport")]
pub fn user_data(name: &str) -> [u8; renet_netcode::NETCODE_USER_DATA_BYTES] {
    let mut out = [0u8; renet_netcode::NETCODE_USER_DATA_BYTES];
    let mut n = 0;
    for c in name.chars() {
        let len = c.len_utf8();
        if n + len > 32 {
            break;
        }
        c.encode_utf8(&mut out[1 + n..]);
        n += len;
    }
    out[0] = n as u8;
    out
}

#[cfg(feature = "transport")]
pub fn name_from_user_data(data: &[u8; renet_netcode::NETCODE_USER_DATA_BYTES]) -> String {
    let n = usize::from(data[0]).min(32);
    String::from_utf8_lossy(&data[1..1 + n]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use survivors_sim::{HeroInput, Sim};

    #[test]
    fn late_snapshot_fits_comfortably() {
        let mut sim = Sim::new(0, 1);
        let ids: Vec<Id> = (0..4)
            .map(|i| sim.add_player("Pat", i * 2, 1, 0, 0).unwrap())
            .collect();
        sim.skip_to(700.0);
        let c = sim.center();
        for _ in 0..300 {
            for (k, id) in ids.iter().enumerate() {
                sim.set_input(
                    *id,
                    HeroInput {
                        pos: Vec3::new(c.x + k as f32 * 3.0, 0.0, c.y),
                        ..HeroInput::default()
                    },
                );
            }
            sim.step(1.0 / 30.0);
            for id in &ids {
                if !sim.offer_of(*id).is_empty() {
                    sim.choose(*id, 0);
                }
            }
            sim.drain_events();
        }
        let bytes = encode(&ServerMsg::Snapshot(Box::new(sim.snapshot())));
        let back: ServerMsg = decode(&bytes).unwrap();
        assert!(matches!(back, ServerMsg::Snapshot(_)));
        eprintln!(
            "late snapshot: {} enemies, {} bytes",
            sim.alive(),
            bytes.len()
        );
        assert!(bytes.len() < 16 * 1024);
        let hero = HeroState::of(&sim, ids[0]).unwrap();
        let run = hero.to_run(None);
        assert_eq!(run.abilities.len(), sim.run_of(ids[0]).unwrap().abilities.len());
    }

    #[test]
    fn names_survive_user_data() {
        assert_eq!(name_from_user_data(&user_data("Thrall")), "Thrall");
        assert_eq!(name_from_user_data(&user_data(&"x".repeat(80))).len(), 32);
    }
}
