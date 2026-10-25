//! What a hero brings into a run from outside it: the gear worn, the talents spent, the account's
//! perks, and the look. The client works it out from the player's account (gear, talent and perk
//! tables live there); the sim only sees plain numbers, so the dedicated server applies a loadout
//! with no tables of its own and checks one by clamping it ([`Loadout::clamped`]).

use serde::{Deserialize, Serialize};

use crate::data;

/// Flat additions to a hero's [`crate::Stats`], from gear, talents and perks together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatBonus {
    /// Added to the damage multiplier (0.10 = +10% damage).
    pub might: f32,
    /// Added maximum health.
    pub health: f32,
    /// Run speed, as a fraction (0.05 = +5%).
    pub speed: f32,
    /// Health regained per second.
    pub regen: f32,
    /// Cooldown recovery, as a fraction.
    pub haste: f32,
    /// Area of effect, as a fraction.
    pub area: f32,
    /// Pickup radius, as a fraction.
    pub magnet: f32,
    /// Experience gained, as a fraction.
    pub growth: f32,
    /// Damage taken off each blow.
    pub armor: f32,
    /// Critical strike chance, as a fraction.
    pub crit: f32,
    /// Gold found, as a fraction.
    pub gold: f32,
}

impl StatBonus {
    pub fn add(&mut self, o: &StatBonus) {
        self.might += o.might;
        self.health += o.health;
        self.speed += o.speed;
        self.regen += o.regen;
        self.haste += o.haste;
        self.area += o.area;
        self.magnet += o.magnet;
        self.growth += o.growth;
        self.armor += o.armor;
        self.crit += o.crit;
        self.gold += o.gold;
    }

    pub fn scaled(&self, k: f32) -> StatBonus {
        StatBonus {
            might: self.might * k,
            health: self.health * k,
            speed: self.speed * k,
            regen: self.regen * k,
            haste: self.haste * k,
            area: self.area * k,
            magnet: self.magnet * k,
            growth: self.growth * k,
            armor: self.armor * k,
            crit: self.crit * k,
            gold: self.gold * k,
        }
    }
}

/// The face and body a player picked: WoW race and sex, and the character-create dials.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub race: u8,
    pub sex: u8,
    pub skin: u8,
    pub face: u8,
    pub hair: u8,
    pub hair_color: u8,
    pub facial: u8,
}

/// Worn equipment slots, in the order the visible-item fields take them.
pub const GEAR_SLOTS: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub stats: StatBonus,
    /// `(ability key, extra damage)`: 0.15 = that spell deals 15% more.
    pub spell_damage: Vec<(String, f32)>,
    /// `(ability key, cooldown cut)`: 0.10 = that spell recovers 10% sooner.
    pub spell_cooldown: Vec<(String, f32)>,
    /// Spells the hero starts every run with, besides the class's starting spell.
    pub start_spells: Vec<String>,
    /// Card tricks a run allows: rerolls, banishes, and whether a hand may be passed for gold.
    pub rerolls: u8,
    pub banishes: u8,
    pub skips: bool,
    /// One blow that would kill instead leaves the hero at half health, once a run.
    pub second_wind: bool,
    pub look: Appearance,
    /// `ItemDisplayInfo` ids worn, one per slot, 0 for none: what teammates see.
    pub gear: [u32; GEAR_SLOTS],
}

/// How far a loadout may push any number: generous for any honest account, a ceiling for a
/// hand-edited one.
mod cap {
    pub const MIGHT: f32 = 3.0;
    pub const HEALTH: f32 = 2000.0;
    pub const FRACTION: f32 = 1.0;
    pub const HASTE: f32 = 0.6;
    pub const REGEN: f32 = 40.0;
    pub const ARMOR: f32 = 60.0;
    pub const CRIT: f32 = 0.6;
    pub const SPELL: f32 = 2.0;
    pub const TRICKS: u8 = 6;
}

impl Loadout {
    /// The loadout with every number held to what an honest account can reach, unknown spells
    /// dropped, and duplicate entries merged.
    pub fn clamped(&self) -> Loadout {
        let c = |v: f32, hi: f32| if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 };
        let s = &self.stats;
        let known = |k: &str| data::ABILITIES.iter().any(|a| a.key == k);
        let merge = |list: &[(String, f32)], hi: f32| {
            let mut out: Vec<(String, f32)> = Vec::new();
            for (k, v) in list.iter().filter(|(k, _)| known(k)) {
                match out.iter_mut().find(|(q, _)| q == k) {
                    Some(e) => e.1 = c(e.1 + v, hi),
                    None => out.push((k.clone(), c(*v, hi))),
                }
            }
            out
        };
        let mut start: Vec<String> = self.start_spells.iter().filter(|k| known(k)).cloned().collect();
        start.dedup();
        start.truncate(2);
        Loadout {
            stats: StatBonus {
                might: c(s.might, cap::MIGHT),
                health: c(s.health, cap::HEALTH),
                speed: c(s.speed, cap::FRACTION),
                regen: c(s.regen, cap::REGEN),
                haste: c(s.haste, cap::HASTE),
                area: c(s.area, cap::FRACTION),
                magnet: c(s.magnet, cap::FRACTION * 2.0),
                growth: c(s.growth, cap::FRACTION),
                armor: c(s.armor, cap::ARMOR),
                crit: c(s.crit, cap::CRIT),
                gold: c(s.gold, cap::FRACTION * 2.0),
            },
            spell_damage: merge(&self.spell_damage, cap::SPELL),
            spell_cooldown: merge(&self.spell_cooldown, 0.5),
            start_spells: start,
            rerolls: self.rerolls.min(cap::TRICKS),
            banishes: self.banishes.min(cap::TRICKS),
            skips: self.skips,
            second_wind: self.second_wind,
            look: self.look,
            gear: self.gear,
        }
    }

    pub fn damage_for(&self, key: &str) -> f32 {
        self.spell_damage
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .sum()
    }

    pub fn cooldown_for(&self, key: &str) -> f32 {
        self.spell_cooldown
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .sum::<f32>()
            .min(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hand_edited_loadout_is_held_to_the_caps() {
        let wild = Loadout {
            stats: StatBonus {
                might: 99.0,
                crit: f32::NAN,
                armor: -5.0,
                ..StatBonus::default()
            },
            spell_damage: vec![
                ("frostbolt".into(), 1.5),
                ("frostbolt".into(), 1.5),
                ("not_a_spell".into(), 1.0),
            ],
            rerolls: 200,
            ..Loadout::default()
        };
        let l = wild.clamped();
        assert_eq!(l.stats.might, cap::MIGHT);
        assert_eq!(l.stats.crit, 0.0);
        assert_eq!(l.stats.armor, 0.0);
        assert_eq!(l.spell_damage, vec![("frostbolt".to_string(), cap::SPELL)]);
        assert_eq!(l.rerolls, cap::TRICKS);
    }
}
