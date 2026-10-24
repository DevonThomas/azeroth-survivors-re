//! A player's own hero on the wire: everything in its [`Run`] the HUD and the cards read.

use serde::{Deserialize, Serialize};
use survivors_sim::data;
use survivors_sim::{Choice, Id, MobilityState, Owned, Run, Sim};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeroState {
    pub id: Id,
    pub class: u8,
    pub race: u8,
    pub elapsed: f32,
    pub kills: u32,
    pub level: u32,
    pub xp: f32,
    pub hp: f32,
    /// `(index into data::ABILITIES, rank, cooldown left)`.
    pub abilities: Vec<(u16, u8, f32)>,
    /// `(index into data::PASSIVES, rank)`.
    pub passives: Vec<(u8, u8)>,
    pub shield: f32,
    pub shield_t: f32,
    pub immune_t: f32,
    pub thorns_t: f32,
    pub heal_t: f32,
    /// Mobility: cooldown, haste, haste left, vanish left.
    pub mobile: [f32; 4],
    pub pending_levels: u32,
    pub bosses: u32,
    pub rings: u32,
    pub won: bool,
    pub dead_t: Option<f32>,
    pub revive: f32,
    pub damage_done: f64,
    pub version: u32,
    pub banner: Option<(String, f32)>,
    /// The cards on offer, `Choice::encode`d.
    pub offer: Vec<(u8, u16)>,
}

impl HeroState {
    pub fn of(sim: &Sim, id: Id) -> Option<Self> {
        let run = sim.run_of(id)?;
        let class = data::CLASSES
            .iter()
            .position(|c| c.id == run.class.id)
            .unwrap_or(0) as u8;
        Some(Self {
            id,
            class,
            race: run.race,
            elapsed: run.elapsed,
            kills: run.kills,
            level: run.level,
            xp: run.xp,
            hp: run.hp,
            abilities: run
                .abilities
                .iter()
                .map(|o| {
                    let i = data::ABILITIES
                        .iter()
                        .position(|a| a.key == o.def.key)
                        .unwrap_or(0);
                    (i as u16, o.level, o.cd)
                })
                .collect(),
            passives: run
                .passives
                .iter()
                .map(|(p, l)| {
                    let i = data::PASSIVES
                        .iter()
                        .position(|q| q.key == p.key)
                        .unwrap_or(0);
                    (i as u8, *l)
                })
                .collect(),
            shield: run.shield,
            shield_t: run.shield_t,
            immune_t: run.immune_t,
            thorns_t: run.thorns_t,
            heal_t: run.heal_t,
            mobile: [
                run.mobile.cd,
                run.mobile.haste,
                run.mobile.haste_t,
                run.mobile.vanish_t,
            ],
            pending_levels: run.pending_levels,
            bosses: run.bosses,
            rings: run.rings,
            won: run.won,
            dead_t: run.dead_t,
            revive: run.revive,
            damage_done: run.damage_done,
            version: run.version,
            banner: run.banner,
            offer: sim.offer_of(id).iter().map(|c| c.encode()).collect(),
        })
    }

    /// The [`Run`] this describes, keeping `prev`'s client-side guid counter.
    pub fn to_run(&self, prev: Option<&Run>) -> Run {
        let class = &data::CLASSES[usize::from(self.class).min(data::CLASSES.len() - 1)];
        Run {
            class,
            race: self.race,
            elapsed: self.elapsed,
            kills: self.kills,
            level: self.level,
            xp: self.xp,
            hp: self.hp,
            abilities: self
                .abilities
                .iter()
                .filter_map(|&(i, level, cd)| {
                    Some(Owned {
                        def: data::ABILITIES.get(usize::from(i))?,
                        level,
                        cd,
                    })
                })
                .collect(),
            passives: self
                .passives
                .iter()
                .filter_map(|&(i, l)| Some((data::PASSIVES.get(usize::from(i))?, l)))
                .collect(),
            shield: self.shield,
            shield_t: self.shield_t,
            immune_t: self.immune_t,
            thorns: 0.0,
            thorns_t: self.thorns_t,
            thorns_spell: 0,
            heal_rate: 0.0,
            heal_t: self.heal_t,
            mobile: MobilityState {
                cd: self.mobile[0],
                haste: self.mobile[1],
                haste_t: self.mobile[2],
                vanish_t: self.mobile[3],
            },
            pending_levels: self.pending_levels,
            bosses: self.bosses,
            rings: self.rings,
            won: self.won,
            dead_t: self.dead_t,
            revive: self.revive,
            next_guid: prev.map_or(1, |r| r.next_guid),
            damage_done: self.damage_done,
            version: self.version,
            banner: self.banner.clone(),
        }
    }

    pub fn offer(&self) -> Vec<Choice> {
        self.offer.iter().filter_map(|&c| Choice::decode(c)).collect()
    }
}
