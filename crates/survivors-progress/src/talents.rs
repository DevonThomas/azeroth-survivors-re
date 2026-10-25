//! Talents: the class's three WoW Classic trees, read by the client from its own Talent.dbc and
//! TalentTab.dbc, each talent given a survivors effect by what it is called. A talent named for
//! one of the class's spells ("Improved Frostbolt") sharpens that spell; a talent that teaches a
//! spell the mode has (Blast Wave, Mortal Strike, Holy Shock) starts every run with it; the rest
//! map by their word to a stat ("Toughness" health, "Cruelty" crit, "Arctic Reach" area), and a
//! talent with no word the mode knows adds damage.

use survivors_sim::{data, StatBonus};

/// One talent, as the client's tables describe it.
#[derive(Clone, Debug, PartialEq)]
pub struct TalentInfo {
    pub id: u32,
    pub name: String,
    /// The tree's name: "Frost", "Arms", "Shadow".
    pub tree: String,
    /// 0-based tier.
    pub row: u8,
    pub col: u8,
    pub max_rank: u8,
    /// The talent teaches a spell rather than modifying one (a single-rank keystone).
    pub teaches: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TalentEffect {
    /// Per rank.
    Stats(StatBonus),
    /// One of the class's spells, per rank: extra damage and cooldown cut.
    Spell {
        key: &'static str,
        damage: f32,
        cooldown: f32,
    },
    /// Every run starts with this spell.
    StartSpell(&'static str),
}

/// The mode's key for a spell name: "Arcane Explosion" -> "arcane_explosion".
fn key_of(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace('\'', "")
        .replace([' ', '-'], "_")
}

/// The class's spell a talent's name points at, if any.
fn class_spell(name: &str, class: u8) -> Option<&'static str> {
    let pool = data::CLASSES.iter().find(|c| c.id == class)?.pool;
    let k = key_of(name);
    let bare = k.strip_prefix("improved_").unwrap_or(&k);
    pool.iter().copied().find(|p| *p == bare)
}

const WORDS: &[(&[&str], fn() -> StatBonus)] = &[
    (
        &["tough", "fortitude", "stamina", "vitality", "endurance", "thick hide", "heart of the wild", "enduring", "iron will", "anticipation"],
        || StatBonus { health: 8.0, ..StatBonus::default() },
    ),
    (
        &["precision", "critical", "cruelty", "malice", "killer", "destruction", "lethality", "conviction", "ruin", "instability", "specialization", "devastation", "impale", "sharpened", "keen"],
        || StatBonus { crit: 0.01, ..StatBonus::default() },
    ),
    (
        &["haste", "quick", "swift", "flurry", "celerity", "speed", "hurry", "agility"],
        || StatBonus { haste: 0.015, ..StatBonus::default() },
    ),
    (
        &["focus", "concentration", "meditation", "wisdom", "tranquil", "spirit", "regenerat", "restorat", "healing", "blessed", "mend"],
        || StatBonus { regen: 0.3, ..StatBonus::default() },
    ),
    (
        &["reach", "range", "expanse", "radius", "nova", "storm", "spread"],
        || StatBonus { area: 0.03, ..StatBonus::default() },
    ),
    (
        &["armor", "shield", "protection", "defense", "deflection", "toughened", "ironclad", "warding", "barrier", "stoneskin", "parry", "dodge", "evasion", "block"],
        || StatBonus { armor: 0.6, ..StatBonus::default() },
    ),
    (
        &["pathfinding", "travel", "feet", "footed", "pursuit", "sprint", "dash"],
        || StatBonus { speed: 0.02, ..StatBonus::default() },
    ),
];

/// What a talent does in the mode.
pub fn effect_for(info: &TalentInfo, class: u8) -> TalentEffect {
    if let Some(key) = class_spell(&info.name, class) {
        // A keystone that teaches a spell the mode has: the run starts with it.
        if info.teaches || !key_of(&info.name).starts_with("improved_") && info.max_rank == 1 {
            return TalentEffect::StartSpell(key);
        }
        let per = 1.0 / f32::from(info.max_rank.max(1));
        return TalentEffect::Spell {
            key,
            damage: 0.25 * per,
            cooldown: 0.15 * per,
        };
    }
    let low = info.name.to_ascii_lowercase();
    for (words, make) in WORDS {
        if words.iter().any(|w| low.contains(w)) {
            return TalentEffect::Stats(make());
        }
    }
    // Deeper talents hit harder: a point at the bottom of a tree is worth more.
    let deep = 1.0 + f32::from(info.row) * 0.25;
    TalentEffect::Stats(StatBonus {
        might: 0.01 * deep,
        ..StatBonus::default()
    })
}

/// Talent points earned with `xp` talent experience: the first comes quickly, each later one
/// costs a little more, up to Classic's 51.
pub fn points_for(xp: u64) -> u32 {
    let mut need = 0u64;
    for p in 0..51u64 {
        need += 60 + 25 * p;
        if xp < need {
            return p as u32;
        }
    }
    51
}

/// The experience for the next point after `points`.
pub fn xp_for_point(points: u32) -> u64 {
    (0..=u64::from(points)).map(|p| 60 + 25 * p).sum()
}

/// Whether a tree at `spent` points opens tier `row` (Classic: five points per tier).
pub fn row_open(row: u8, spent: u32) -> bool {
    u32::from(row) * 5 <= spent
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, row: u8, max_rank: u8) -> TalentInfo {
        TalentInfo {
            id: 1,
            name: name.into(),
            tree: "Frost".into(),
            row,
            col: 0,
            max_rank,
            teaches: false,
        }
    }

    #[test]
    fn classic_talents_map_to_sensible_effects() {
        // Mage (8).
        assert!(matches!(
            effect_for(&t("Improved Frostbolt", 0, 5), 8),
            TalentEffect::Spell { key: "frostbolt", .. }
        ));
        assert_eq!(effect_for(&t("Blast Wave", 6, 1), 8), TalentEffect::StartSpell("blast_wave"));
        assert!(matches!(effect_for(&t("Arctic Reach", 2, 2), 8), TalentEffect::Stats(s) if s.area > 0.0));
        // Warrior (1).
        assert_eq!(effect_for(&t("Mortal Strike", 6, 1), 1), TalentEffect::StartSpell("mortal_strike"));
        assert!(matches!(effect_for(&t("Cruelty", 1, 5), 1), TalentEffect::Stats(s) if s.crit > 0.0));
        assert!(matches!(effect_for(&t("Toughness", 2, 5), 1), TalentEffect::Stats(s) if s.health > 0.0));
        // Anything else still counts.
        assert!(matches!(effect_for(&t("Shadow Weaving", 3, 5), 5), TalentEffect::Stats(s) if s.might > 0.0));
    }

    #[test]
    fn talent_points_grow_to_fifty_one() {
        assert_eq!(points_for(0), 0);
        assert_eq!(points_for(60), 1);
        assert_eq!(points_for(u64::MAX), 51);
        assert!(points_for(xp_for_point(9)) == 10);
        assert!(row_open(0, 0) && row_open(2, 10) && !row_open(3, 14));
    }
}
