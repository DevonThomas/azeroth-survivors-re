//! Azeroth Survivors' rules, with no engine under them. A [`Sim`] is one run: up to
//! [`MAX_PLAYERS`] heroes, the horde, the spells in flight and the clock. It works on the ground
//! plane only (Bevy's x and z, as a [`Vec2`]); the client seats everything on its terrain.
//!
//! The client steps a `Sim` itself for a solo run. For co-op the dedicated server steps it and
//! streams a [`Snapshot`] and the [`Event`]s to every player, so both paths draw from the same
//! two types.

pub mod data;
mod grid;
mod loadout;
mod offer;
mod rng;
mod run;
mod sim;
mod snapshot;
mod speeds;

pub use glam::{Vec2, Vec3};
pub use loadout::{Appearance, Loadout, StatBonus, GEAR_SLOTS};
pub use offer::{choices_for, Choice};
pub use rng::Rng;
pub use run::{mobility_for, Mobility, MobilityState, Move, Owned, Run, Stats, LEARN_LEVEL};
pub use run::{BASE_HP, BASE_MAGNET, BASE_SPEED};
pub use sim::{HeroInput, Sim, SimPhase, MAX_PLAYERS, REVIVE_RADIUS, REVIVE_SECS};
pub use speeds::MISSILE_SPEEDS;
pub use snapshot::{
    AreaSnap, EnemySnap, Event, GemLook, GemSnap, PlayerSnap, Snapshot, TotemSnap,
};

/// A thing in the run, unique for the run's life: a hero, an enemy, a gem, a totem, an area.
/// Heroes take `1..=MAX_PLAYERS`, everything else counts up from [`FIRST_SPAWN_ID`].
pub type Id = u32;

pub const FIRST_SPAWN_ID: Id = 16;

/// The ground-plane point of a Bevy position.
pub fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

/// The Bevy yaw that faces a model along `dir` on the ground plane.
pub fn yaw_toward(dir: Vec2) -> f32 {
    (-dir.x).atan2(-dir.y)
}

/// Floating-number colours, 0xAARRGGBB.
pub const COLOR_SPELL: u32 = 0xFFFF_DE00;
pub const COLOR_MELEE: u32 = 0xFFFF_FFFF;
pub const COLOR_HURT: u32 = 0xFFFF_3030;
pub const COLOR_HEAL: u32 = 0xFF30_FF30;
