//! Account progression for Azeroth Survivors: what carries over between runs. A player keeps one
//! [`Account`]: gold, an item collection, each class's worn gear, talents and look, the
//! account-wide perks, and achievements. [`loadout`] adds it all up into the
//! [`survivors_sim::Loadout`] a run starts with; [`Account::finish_run`] pays a run out.
//!
//! Everything here is built from World of Warcraft Classic itself: the gear is each class's four
//! Classic armour sets (the client draws them from its own art, [`gear`]), the talent trees are the
//! client's own Talent.dbc read at runtime with each talent given an effect by its name
//! ([`talents`]), and the perks and achievements are this game's own.

pub mod account;
pub mod achievements;
pub mod gear;
pub mod perks;
pub mod talents;

pub use account::{Account, ClassProgress, RunResult};
pub use achievements::{Achievement, ACHIEVEMENTS};
pub use gear::{ItemKey, Slot, Tier, ARMOUR_SLOTS};
pub use perks::{Perk, PERKS};
pub use talents::{TalentEffect, TalentInfo};

use survivors_sim::{Loadout, StatBonus, GEAR_SLOTS};

/// Everything the account brings to a run as `class` (a WoW class id): worn gear and its set
/// bonuses, spent talents, bought perks, and the look. `talent_info` describes a talent by id (the
/// client reads Talent.dbc); `display` gives the art a worn item draws with, by gear slot.
pub fn loadout(
    account: &Account,
    class: u8,
    talent_info: &dyn Fn(u32) -> Option<TalentInfo>,
    display: &dyn Fn(ItemKey) -> Option<u32>,
) -> Loadout {
    let mut out = Loadout::default();
    let Some(cp) = account.classes.get(&class) else {
        out.stats = perks::stats(&account.perks);
        perks::tricks(&account.perks, &mut out);
        return out;
    };
    let mut stats = StatBonus::default();

    // Gear and its set bonuses.
    let mut gear = [0u32; GEAR_SLOTS];
    for (&slot, &item) in &cp.equipped {
        stats.add(&gear::stats(item));
        if let Some(s) = Slot::from_index(slot) {
            if let Some(d) = display(item) {
                gear[s.gear_index()] = d;
            }
        }
    }
    stats.add(&gear::set_bonus(cp.equipped.values().copied()));

    // Talents.
    for (&id, &rank) in &cp.talents {
        let Some(info) = talent_info(id) else { continue };
        let rank = rank.min(info.max_rank);
        match talents::effect_for(&info, class) {
            TalentEffect::Stats(per) => stats.add(&per.scaled(f32::from(rank))),
            TalentEffect::Spell { key, damage, cooldown } => {
                if damage > 0.0 {
                    out.spell_damage.push((key.to_string(), damage * f32::from(rank)));
                }
                if cooldown > 0.0 {
                    out.spell_cooldown.push((key.to_string(), cooldown * f32::from(rank)));
                }
            }
            TalentEffect::StartSpell(key) => {
                if rank > 0 && !out.start_spells.iter().any(|k| k == key) {
                    out.start_spells.push(key.to_string());
                }
            }
        }
    }

    // Perks: the account's, for every class.
    stats.add(&perks::stats(&account.perks));
    perks::tricks(&account.perks, &mut out);

    out.stats = stats;
    out.look = cp.look;
    out.gear = gear;
    out
}
