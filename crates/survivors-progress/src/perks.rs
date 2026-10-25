//! Perks: the account's own upgrades, for every hero of every class, bought with perk points (one
//! for each achievement earned) and refunded freely.

use std::collections::BTreeMap;

use survivors_sim::{Loadout, StatBonus};

pub struct Perk {
    pub key: &'static str,
    pub name: &'static str,
    /// A WoW Classic spell for the icon.
    pub icon_spell: u32,
    pub ranks: u8,
    /// Points for each rank.
    pub cost: u32,
    pub desc: &'static str,
    kind: Kind,
}

enum Kind {
    Stat(fn(f32) -> StatBonus),
    Rerolls,
    Banishes,
    Skips,
    SecondWind,
}

pub static PERKS: &[Perk] = &[
    Perk {
        key: "hardy",
        name: "Hardy",
        icon_spell: 1243,
        ranks: 5,
        cost: 1,
        desc: "+15 maximum health per rank.",
        kind: Kind::Stat(|r| StatBonus { health: 15.0 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "honed",
        name: "Honed",
        icon_spell: 6673,
        ranks: 5,
        cost: 1,
        desc: "+2% damage per rank.",
        kind: Kind::Stat(|r| StatBonus { might: 0.02 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "keen_eye",
        name: "Keen Eye",
        icon_spell: 19426,
        ranks: 3,
        cost: 2,
        desc: "+1% critical strike chance per rank.",
        kind: Kind::Stat(|r| StatBonus { crit: 0.01 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "fleet",
        name: "Fleet",
        icon_spell: 13159,
        ranks: 3,
        cost: 1,
        desc: "+3% run speed per rank.",
        kind: Kind::Stat(|r| StatBonus { speed: 0.03 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "studious",
        name: "Studious",
        icon_spell: 1459,
        ranks: 3,
        cost: 1,
        desc: "+5% experience per rank.",
        kind: Kind::Stat(|r| StatBonus { growth: 0.05 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "prospector",
        name: "Prospector",
        icon_spell: 921,
        ranks: 3,
        cost: 1,
        desc: "+10% gold found per rank.",
        kind: Kind::Stat(|r| StatBonus { gold: 0.10 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "long_arms",
        name: "Long Arms",
        icon_spell: 6197,
        ranks: 3,
        cost: 1,
        desc: "+10% pickup radius per rank.",
        kind: Kind::Stat(|r| StatBonus { magnet: 0.10 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "mending",
        name: "Mending",
        icon_spell: 139,
        ranks: 3,
        cost: 1,
        desc: "Regenerate 0.4 health a second per rank.",
        kind: Kind::Stat(|r| StatBonus { regen: 0.4 * r, ..StatBonus::default() }),
    },
    Perk {
        key: "fickle_fortune",
        name: "Fickle Fortune",
        icon_spell: 23028,
        ranks: 3,
        cost: 2,
        desc: "One more card reroll every run, per rank.",
        kind: Kind::Rerolls,
    },
    Perk {
        key: "exile",
        name: "Exile",
        icon_spell: 710,
        ranks: 3,
        cost: 2,
        desc: "Once a run per rank, banish a card: it is never dealt again that run.",
        kind: Kind::Banishes,
    },
    Perk {
        key: "haggler",
        name: "Haggler",
        icon_spell: 1804,
        ranks: 1,
        cost: 2,
        desc: "Pass on any hand of cards and take gold instead.",
        kind: Kind::Skips,
    },
    Perk {
        key: "defiant",
        name: "Defiant",
        icon_spell: 12975,
        ranks: 1,
        cost: 3,
        desc: "Once a run, a blow that would kill you leaves you at half health.",
        kind: Kind::SecondWind,
    },
];

pub fn perk(key: &str) -> Option<&'static Perk> {
    PERKS.iter().find(|p| p.key == key)
}

/// The stat bonuses of the bought ranks.
pub fn stats(bought: &BTreeMap<String, u8>) -> StatBonus {
    let mut s = StatBonus::default();
    for (key, &rank) in bought {
        if let Some(p) = perk(key) {
            if let Kind::Stat(f) = p.kind {
                s.add(&f(f32::from(rank.min(p.ranks))));
            }
        }
    }
    s
}

/// The card tricks and the second wind the bought perks give.
pub fn tricks(bought: &BTreeMap<String, u8>, out: &mut Loadout) {
    for (key, &rank) in bought {
        let Some(p) = perk(key) else { continue };
        let rank = rank.min(p.ranks);
        match p.kind {
            Kind::Rerolls => out.rerolls += rank,
            Kind::Banishes => out.banishes += rank,
            Kind::Skips => out.skips |= rank > 0,
            Kind::SecondWind => out.second_wind |= rank > 0,
            Kind::Stat(_) => {}
        }
    }
}

/// Points spent on `bought`.
pub fn spent(bought: &BTreeMap<String, u8>) -> u32 {
    bought
        .iter()
        .filter_map(|(k, &r)| perk(k).map(|p| p.cost * u32::from(r.min(p.ranks))))
        .sum()
}
