//! Gear: each class's four WoW Classic armour sets, one per stage of the zone ladder. A boss drops
//! a piece of the set for its zone's stage; worn pieces add stats, and wearing several of one set
//! adds its bonus. The pieces' looks are the client's own art for those sets (`Dungeon*_A`,
//! `Raid*_A/B/C` in ItemDisplayInfo), found at runtime by the client.

use serde::{Deserialize, Serialize};
use survivors_sim::StatBonus;

/// The armour slots a set fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Slot {
    Head,
    Shoulders,
    Chest,
    Wrists,
    Hands,
    Waist,
    Legs,
    Feet,
    Back,
    Finger,
    MainHand,
}

/// Every slot a boss can drop for.
pub const ALL_SLOTS: [Slot; 11] = [
    Slot::Head,
    Slot::Shoulders,
    Slot::Chest,
    Slot::Wrists,
    Slot::Hands,
    Slot::Waist,
    Slot::Legs,
    Slot::Feet,
    Slot::Back,
    Slot::Finger,
    Slot::MainHand,
];

/// The slots a set's pieces fill (cloaks, rings and weapons belong to no set).
pub const ARMOUR_SLOTS: [Slot; 8] = [
    Slot::Head,
    Slot::Shoulders,
    Slot::Chest,
    Slot::Wrists,
    Slot::Hands,
    Slot::Waist,
    Slot::Legs,
    Slot::Feet,
];

impl Slot {
    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(i: u8) -> Option<Slot> {
        ALL_SLOTS.get(usize::from(i)).copied()
    }

    pub fn is_armour(self) -> bool {
        ARMOUR_SLOTS.contains(&self)
    }

    /// Where the slot sits in a [`survivors_sim::Loadout`]'s `gear` (the visible-item order:
    /// head, shoulders, chest, waist, legs, feet, wrists, hands, main hand, off hand, back);
    /// `None` for a ring, which the body does not draw.
    pub fn gear_index(self) -> Option<usize> {
        Some(match self {
            Slot::Head => 0,
            Slot::Shoulders => 1,
            Slot::Chest => 2,
            Slot::Waist => 3,
            Slot::Legs => 4,
            Slot::Feet => 5,
            Slot::Wrists => 6,
            Slot::Hands => 7,
            Slot::MainHand => 8,
            Slot::Back => 10,
            Slot::Finger => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Slot::Head => "Head",
            Slot::Shoulders => "Shoulders",
            Slot::Chest => "Chest",
            Slot::Wrists => "Wrists",
            Slot::Hands => "Hands",
            Slot::Waist => "Waist",
            Slot::Legs => "Legs",
            Slot::Feet => "Feet",
            Slot::Back => "Back",
            Slot::Finger => "Finger",
            Slot::MainHand => "Main hand",
        }
    }

    /// How much of a set's power the slot carries.
    fn weight(self) -> f32 {
        match self {
            Slot::Chest | Slot::Legs => 1.0,
            Slot::Head | Slot::Shoulders => 0.8,
            Slot::Hands | Slot::Feet | Slot::Waist => 0.6,
            Slot::Wrists => 0.45,
            Slot::Back | Slot::Finger => 0.5,
            Slot::MainHand => 1.6,
        }
    }
}

/// The four sets, in the order the zone ladder reaches them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Dungeon Set 1.
    Dungeon,
    /// Tier 1, Molten Core.
    Raid1,
    /// Tier 2, Blackwing Lair.
    Raid2,
    /// Tier 3, Naxxramas.
    Raid3,
}

pub const TIERS: [Tier; 4] = [Tier::Dungeon, Tier::Raid1, Tier::Raid2, Tier::Raid3];

impl Tier {
    pub fn rank(self) -> f32 {
        match self {
            Tier::Dungeon => 1.0,
            Tier::Raid1 => 2.0,
            Tier::Raid2 => 3.0,
            Tier::Raid3 => 4.0,
        }
    }

    /// The set a boss on battleground `map` (an index into the zone ladder) drops from.
    pub fn for_map(map: usize) -> Tier {
        match map {
            0..=6 => Tier::Dungeon,
            7..=10 => Tier::Raid1,
            11..=13 => Tier::Raid2,
            _ => Tier::Raid3,
        }
    }

    /// The art family's name in the client's textures: `Dungeon<Class>_A`, `Raid<Class>_A/B/C`.
    pub fn art(self, class: u8) -> Option<String> {
        let who = class_art(class)?;
        Some(match self {
            Tier::Dungeon => format!("dungeon{who}_a"),
            Tier::Raid1 => format!("raid{who}_a"),
            Tier::Raid2 => format!("raid{who}_b"),
            Tier::Raid3 => format!("raid{who}_c"),
        })
    }

    /// Where the tier's pieces come from, for the pieces outside the sets.
    pub fn source(self) -> &'static str {
        match self {
            Tier::Dungeon => "Dungeon",
            Tier::Raid1 => "Molten Core",
            Tier::Raid2 => "Blackwing Lair",
            Tier::Raid3 => "Naxxramas",
        }
    }

    /// The rarity a piece shows in: blue dungeon gear, purple raid gear.
    pub fn quality(self) -> u8 {
        match self {
            Tier::Dungeon => 3,
            _ => 4,
        }
    }
}

fn class_art(class: u8) -> Option<&'static str> {
    Some(match class {
        1 => "warrior",
        2 => "paladin",
        3 => "hunter",
        4 => "rogue",
        5 => "priest",
        7 => "shaman",
        8 => "mage",
        9 => "warlock",
        11 => "druid",
        _ => return None,
    })
}

/// The Classic set names, `[Dungeon 1, Tier 1, Tier 2, Tier 3]`, by WoW class id.
pub fn set_names(class: u8) -> [&'static str; 4] {
    match class {
        1 => ["Battlegear of Valor", "Battlegear of Might", "Battlegear of Wrath", "Dreadnaught's Battlegear"],
        2 => ["Lightforge Armor", "Lawbringer Armor", "Judgement Armor", "Redemption Armor"],
        3 => ["Beaststalker Armor", "Giantstalker Armor", "Dragonstalker Armor", "Cryptstalker Armor"],
        4 => ["Shadowcraft Armor", "Nightslayer Armor", "Bloodfang Armor", "Bonescythe Armor"],
        5 => ["Vestments of the Devout", "Vestments of Prophecy", "Vestments of Transcendence", "Vestments of Faith"],
        7 => ["The Elements", "The Earthfury", "The Ten Storms", "The Earthshatterer"],
        8 => ["Magister's Regalia", "Arcanist Regalia", "Netherwind Regalia", "Frostfire Regalia"],
        9 => ["Dreadmist Raiment", "Felheart Raiment", "Nemesis Raiment", "Plagueheart Raiment"],
        11 => ["Wildheart Raiment", "Cenarion Raiment", "Stormrage Raiment", "Dreamwalker Raiment"],
        _ => ["", "", "", ""],
    }
}

/// One piece: a class's set piece for one slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemKey {
    pub class: u8,
    pub tier: Tier,
    pub slot: Slot,
}

impl ItemKey {
    /// "Arcanist Regalia: Head", "Molten Core Staff".
    pub fn name(self) -> String {
        match self.slot {
            Slot::Back => format!("{} Cloak", self.tier.source()),
            Slot::Finger => format!("{} Band", self.tier.source()),
            Slot::MainHand => format!("{} {}", self.tier.source(), weapon_noun(self.class)),
            _ => format!(
                "{}: {}",
                set_names(self.class)[tier_index(self.tier)],
                self.slot.name()
            ),
        }
    }
}

/// What the class swings, as its default weapon is.
fn weapon_noun(class: u8) -> &'static str {
    match class {
        1 | 4 => "Blade",
        2 | 7 => "Mace",
        3 | 5 | 8 | 9 | 11 => "Staff",
        _ => "Weapon",
    }
}

/// Each enhancement level adds this much of the piece's stats.
pub const ENHANCE_STEP: f32 = 0.15;
pub const MAX_ENHANCE: u8 = 5;

/// Gold to take a piece from `level` to `level + 1`.
pub fn enhance_cost(item: ItemKey, level: u8) -> u64 {
    let l = u64::from(level) + 1;
    (40.0 * item.tier.rank() * item.slot.weight()) as u64 * l * l + 20
}

/// What a piece adds at an enhancement level.
pub fn stats_at(item: ItemKey, level: u8) -> StatBonus {
    stats(item).scaled(1.0 + ENHANCE_STEP * f32::from(level.min(MAX_ENHANCE)))
}

fn tier_index(t: Tier) -> usize {
    TIERS.iter().position(|x| *x == t).unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Mage, warlock, priest, shaman, druid: damage, crit and haste.
    Caster,
    /// Warrior, paladin, rogue: damage, health and armour.
    Melee,
    /// Hunter: damage, crit and speed.
    Ranged,
}

fn role(class: u8) -> Role {
    match class {
        1 | 2 | 4 => Role::Melee,
        3 => Role::Ranged,
        _ => Role::Caster,
    }
}

/// What a piece adds.
pub fn stats(item: ItemKey) -> StatBonus {
    let k = item.tier.rank() * item.slot.weight();
    let mut s = StatBonus {
        might: 0.025 * k,
        health: 6.0 * k,
        ..StatBonus::default()
    };
    match role(item.class) {
        Role::Caster => {
            s.crit = 0.004 * k;
            s.haste = 0.006 * k;
        }
        Role::Melee => {
            s.health += 10.0 * k;
            s.armor = 0.5 * k;
        }
        Role::Ranged => {
            s.crit = 0.005 * k;
            s.speed = 0.006 * k;
        }
    }
    s
}

/// The bonus for wearing pieces of one set together: at 2, 4 and 8 pieces of the same set.
pub fn set_bonus(worn: impl Iterator<Item = ItemKey>) -> StatBonus {
    let mut counts = [0u32; 4];
    for item in worn.filter(|i| i.slot.is_armour()) {
        counts[tier_index(item.tier)] += 1;
    }
    let mut s = StatBonus::default();
    for (i, &n) in counts.iter().enumerate() {
        let k = (i + 1) as f32;
        if n >= 2 {
            s.regen += 0.5 * k;
        }
        if n >= 4 {
            s.might += 0.04 * k;
        }
        if n >= 8 {
            s.crit += 0.02 * k;
            s.area += 0.05 * k;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_sets_are_stronger_and_names_read_right() {
        let a = ItemKey {
            class: 8,
            tier: Tier::Dungeon,
            slot: Slot::Chest,
        };
        let b = ItemKey {
            tier: Tier::Raid3,
            ..a
        };
        assert!(stats(b).might > stats(a).might);
        assert_eq!(b.name(), "Frostfire Regalia: Chest");
        assert_eq!(Tier::for_map(0), Tier::Dungeon);
        assert_eq!(Tier::for_map(15), Tier::Raid3);
        assert_eq!(a.tier.art(8).as_deref(), Some("dungeonmage_a"));
    }

    #[test]
    fn a_full_set_earns_every_bonus() {
        let full = ARMOUR_SLOTS.map(|slot| ItemKey {
            class: 1,
            tier: Tier::Raid1,
            slot,
        });
        let b = set_bonus(full.into_iter());
        assert!(b.regen > 0.0 && b.might > 0.0 && b.crit > 0.0);
    }
}
