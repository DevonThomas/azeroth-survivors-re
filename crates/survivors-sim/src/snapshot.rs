//! What a client sees of a [`crate::Sim`]: a [`Snapshot`] of everything standing on the field
//! and the [`Event`]s since the last one. Both are the wire types too, so the field types are
//! sized for a packet: enemies and gems are quantized against the battleground's centre.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::Id;

/// Ground-plane quantum: 1/64 yd, ±512 yd around the battleground's centre.
const Q: f32 = 64.0;

pub(crate) fn quantize(p: Vec2, origin: Vec2) -> [i16; 2] {
    let d = (p - origin) * Q;
    [
        d.x.round().clamp(-32768.0, 32767.0) as i16,
        d.y.round().clamp(-32768.0, 32767.0) as i16,
    ]
}

pub(crate) fn dequantize(q: [i16; 2], origin: Vec2) -> Vec2 {
    origin + Vec2::new(f32::from(q[0]), f32::from(q[1])) / Q
}

pub(crate) fn yaw_byte(yaw: f32) -> u8 {
    let t = yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    (t * 256.0).round() as u32 as u8
}

pub(crate) fn byte_yaw(b: u8) -> f32 {
    f32::from(b) / 256.0 * std::f32::consts::TAU
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u32,
    pub elapsed: f32,
    /// The battleground's centre on the ground plane: the origin the quantized points hang off.
    pub origin: Vec2,
    pub map: u16,
    pub level: u32,
    pub xp: f32,
    pub xp_next: f32,
    pub kills: u32,
    pub bosses: u32,
    /// 0 playing, 1 the party is picking cards, 2 over.
    pub phase: u8,
    pub won: bool,
    pub players: Vec<PlayerSnap>,
    pub enemies: Vec<EnemySnap>,
    pub gems: Vec<GemSnap>,
    pub totems: Vec<TotemSnap>,
    pub areas: Vec<AreaSnap>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSnap {
    pub id: Id,
    pub name: String,
    pub class: u8,
    pub race: u8,
    pub sex: u8,
    pub display: u32,
    pub pos: Vec3,
    pub yaw: f32,
    /// `MovementState` flags and speed, so another player's hero runs, strafes and jumps.
    pub flags: u32,
    pub speed: f32,
    pub vy: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub level: u32,
    pub down: bool,
    pub revive: f32,
    /// Spells whose state kits show on the hero (shield, immunity, orbs).
    pub buffs: Vec<u32>,
    pub kills: u32,
    pub damage: f64,
    /// Still choosing a card.
    pub picking: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnemySnap {
    pub id: Id,
    /// Index into the battleground's roster; the top bit marks a boss.
    pub kind: u16,
    pub pos: [i16; 2],
    pub yaw: u8,
    /// Run speed in 1/16 yd/s, 0 standing.
    pub speed: u8,
    /// Health, 0..=255 of max.
    pub hp: u8,
    pub dead: bool,
    pub marks: Vec<u32>,
}

impl EnemySnap {
    pub const BOSS: u16 = 0x8000;

    pub fn boss(&self) -> bool {
        self.kind & Self::BOSS != 0
    }
    pub fn roster_index(&self) -> usize {
        usize::from(self.kind & !Self::BOSS)
    }
    pub fn point(&self, origin: Vec2) -> Vec2 {
        dequantize(self.pos, origin)
    }
    pub fn yaw(&self) -> f32 {
        byte_yaw(self.yaw)
    }
    pub fn run_speed(&self) -> f32 {
        f32::from(self.speed) / 16.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GemLook {
    Small,
    Green,
    Purple,
    Heal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GemSnap {
    pub id: Id,
    pub look: GemLook,
    pub pos: [i16; 2],
    /// Flying to this hero.
    pub to: Option<Id>,
}

impl GemSnap {
    pub fn point(&self, origin: Vec2) -> Vec2 {
        dequantize(self.pos, origin)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TotemSnap {
    pub id: Id,
    pub display: u32,
    pub pos: Vec2,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AreaSnap {
    pub id: Id,
    pub spell: u32,
    pub radius: f32,
    pub pos: Vec2,
    /// The spell's own visual would hang over the field: the client shows no area object.
    pub overhead: bool,
}

/// Something that happened, for the client to show.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    /// `SMSG_SPELL_GO`: the cast kit on the caster, a missile or impact at each target.
    Cast {
        caster: Id,
        spell: u32,
        targets: Vec<Id>,
        dest: Option<Vec2>,
    },
    /// Only the spell's impact kit, on `target`.
    Impact { target: Id, spell: u32 },
    /// A melee swing.
    Swing { attacker: Id, victim: Id, damage: u32 },
    /// A floating number over `anchor`.
    Number {
        anchor: Id,
        amount: u32,
        crit: bool,
        color: u32,
    },
    Banner { text: String, secs: f32 },
    /// The sim moved a hero (Blink, Charge): its own client puts it there.
    Teleport {
        player: Id,
        to: Vec2,
        yaw: f32,
        seq: u32,
    },
    Downed { player: Id },
    Revived { player: Id, by: Id },
}

impl Event {
    /// Worth resending until it lands: everything else is look, and a lost one is not missed.
    pub fn reliable(&self) -> bool {
        matches!(
            self,
            Self::Banner { .. } | Self::Teleport { .. } | Self::Downed { .. } | Self::Revived { .. }
        )
    }
}
