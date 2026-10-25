//! The account: what one player keeps between runs, saved as JSON.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use survivors_sim::{Appearance, Rng};

use crate::gear::{ItemKey, Slot, Tier, ARMOUR_SLOTS};
use crate::{achievements, perks, talents};

/// Lifetime totals across every run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lifetime {
    pub runs: u64,
    pub wins: u64,
    pub kills: u64,
    pub bosses: u64,
    pub gold: u64,
    pub best_level: u32,
}

/// One class's hero.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClassProgress {
    pub look: Appearance,
    /// Worn, by [`Slot::index`].
    pub equipped: BTreeMap<u8, ItemKey>,
    pub talent_xp: u64,
    /// Talent id (Talent.dbc) to rank.
    pub talents: BTreeMap<u32, u8>,
    /// Battlegrounds won with this class, by index.
    pub won: BTreeSet<u16>,
}

impl ClassProgress {
    pub fn talent_points(&self) -> u32 {
        talents::points_for(self.talent_xp)
    }

    pub fn talent_points_spent(&self) -> u32 {
        self.talents.values().map(|&r| u32::from(r)).sum()
    }

    pub fn talent_points_free(&self) -> u32 {
        self.talent_points().saturating_sub(self.talent_points_spent())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub version: u32,
    pub gold: u64,
    /// By WoW class id.
    pub classes: BTreeMap<u8, ClassProgress>,
    pub collection: BTreeSet<ItemKey>,
    pub perks: BTreeMap<String, u8>,
    pub achievements: BTreeSet<String>,
    pub lifetime: Lifetime,
}

/// How a run went, for the account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunResult {
    pub class: u8,
    pub map: u16,
    pub won: bool,
    pub level: u32,
    pub kills: u32,
    pub bosses: u32,
    pub gold: u32,
    pub seconds: f32,
}

/// What a finished run paid out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Payout {
    pub talent_xp: u64,
    pub new_points: u32,
    pub gold: u32,
    pub new_achievements: Vec<String>,
}

impl Account {
    pub const VERSION: u32 = 1;

    pub fn from_json(text: &str) -> Option<Account> {
        serde_json::from_str(text).ok()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn class(&mut self, class: u8) -> &mut ClassProgress {
        self.classes.entry(class).or_default()
    }

    /// A boss of battleground `map` fell: the piece `class` gets, a slot of that zone's set it
    /// does not own yet where there is one. Added to the collection.
    pub fn roll_drop(&mut self, class: u8, map: usize, rng: &mut Rng) -> ItemKey {
        let tier = Tier::for_map(map);
        let missing: Vec<Slot> = ARMOUR_SLOTS
            .into_iter()
            .filter(|&slot| !self.collection.contains(&ItemKey { class, tier, slot }))
            .collect();
        let pool = if missing.is_empty() {
            ARMOUR_SLOTS.to_vec()
        } else {
            missing
        };
        let item = ItemKey {
            class,
            tier,
            slot: pool[rng.index(pool.len())],
        };
        self.collection.insert(item);
        item
    }

    /// Wear `item` (from the collection) in its slot; `false` if it is not owned.
    pub fn equip(&mut self, item: ItemKey) -> bool {
        if !self.collection.contains(&item) {
            return false;
        }
        self.class(item.class)
            .equipped
            .insert(item.slot.index(), item);
        true
    }

    pub fn unequip(&mut self, class: u8, slot: Slot) {
        self.class(class).equipped.remove(&slot.index());
    }

    /// Spend a point on talent `id` of a tree, if a point is free, the tier is open (`tree_spent`
    /// points already in that tree) and the talent is not maxed.
    pub fn learn_talent(&mut self, class: u8, id: u32, row: u8, max_rank: u8, tree_spent: u32) -> bool {
        let cp = self.class(class);
        let rank = cp.talents.get(&id).copied().unwrap_or(0);
        if cp.talent_points_free() == 0 || rank >= max_rank || !talents::row_open(row, tree_spent) {
            return false;
        }
        cp.talents.insert(id, rank + 1);
        true
    }

    /// Take every talent point back.
    pub fn reset_talents(&mut self, class: u8) {
        self.class(class).talents.clear();
    }

    pub fn perk_points(&self) -> u32 {
        self.achievements.len() as u32
    }

    pub fn perk_points_free(&self) -> u32 {
        self.perk_points().saturating_sub(perks::spent(&self.perks))
    }

    pub fn buy_perk(&mut self, key: &str) -> bool {
        let Some(p) = perks::perk(key) else { return false };
        let rank = self.perks.get(key).copied().unwrap_or(0);
        if rank >= p.ranks || self.perk_points_free() < p.cost {
            return false;
        }
        self.perks.insert(key.to_string(), rank + 1);
        true
    }

    pub fn reset_perks(&mut self) {
        self.perks.clear();
    }

    /// Bank a finished run: talent experience for the class, gold, lifetime totals, the win, and
    /// any achievements newly earned.
    pub fn finish_run(&mut self, r: &RunResult) -> Payout {
        let xp = u64::from(r.level) * 10
            + u64::from(r.kills) / 4
            + u64::from(r.bosses) * 60
            + (r.seconds as u64) / 6
            + if r.won { 400 } else { 0 };
        let cp = self.class(r.class);
        let before = cp.talent_points();
        cp.talent_xp += xp;
        if r.won {
            cp.won.insert(r.map);
        }
        let new_points = cp.talent_points() - before;
        self.gold += u64::from(r.gold);
        let l = &mut self.lifetime;
        l.runs += 1;
        l.wins += u64::from(r.won);
        l.kills += u64::from(r.kills);
        l.bosses += u64::from(r.bosses);
        l.gold += u64::from(r.gold);
        l.best_level = l.best_level.max(r.level);
        Payout {
            talent_xp: xp,
            new_points,
            gold: r.gold,
            new_achievements: self.award_achievements(),
        }
    }

    /// Mark every achievement now earned; returns the new ones' names.
    pub fn award_achievements(&mut self) -> Vec<String> {
        let new: Vec<(String, String)> = achievements::ACHIEVEMENTS
            .iter()
            .filter(|a| !self.achievements.contains(&a.key) && a.earned(self))
            .map(|a| (a.key.clone(), a.name.clone()))
            .collect();
        for (k, _) in &new {
            self.achievements.insert(k.clone());
        }
        new.into_iter().map(|(_, n)| n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{loadout, TalentInfo};

    #[test]
    fn a_season_of_runs_builds_a_stronger_hero() {
        let mut acc = Account::default();
        let mut rng = Rng::seeded(4);
        for i in 0..12 {
            let p = acc.finish_run(&RunResult {
                class: 8,
                map: (i % 3) as u16,
                won: i % 4 == 3,
                level: 20 + i,
                kills: 900,
                bosses: 3,
                gold: 250,
                seconds: 600.0,
            });
            assert!(p.talent_xp > 0);
            for _ in 0..3 {
                let item = acc.roll_drop(8, 0, &mut rng);
                acc.equip(item);
            }
        }
        assert!(acc.achievements.contains("runs_1"));
        assert!(acc.collection.len() >= 8, "a dungeon set fills in over a season");
        assert!(acc.class(8).talent_points() >= 20);

        // Spend talents across a toy tree and buy perks.
        let infos: Vec<TalentInfo> = (0..7u8)
            .map(|row| TalentInfo {
                id: 100 + u32::from(row),
                name: if row == 6 { "Blast Wave".into() } else { "Improved Fireball".into() },
                tree: "Fire".into(),
                row,
                col: 0,
                max_rank: if row == 6 { 1 } else { 5 },
                teaches: row == 6,
            })
            .collect();
        let mut spent = 0;
        for t in &infos {
            while acc.learn_talent(8, t.id, t.row, t.max_rank, spent) {
                spent += 1;
            }
        }
        while acc.buy_perk("honed") || acc.buy_perk("hardy") {}
        let lo = loadout(
            &acc,
            8,
            &|id| infos.iter().find(|t| t.id == id).cloned(),
            &|_| Some(1),
        );
        assert!(lo.stats.might > 0.1, "gear and perks add damage");
        assert!(lo.spell_damage.iter().any(|(k, _)| k == "fireball"));
        assert!(lo.start_spells.contains(&"blast_wave".to_string()) || spent < 31);
        assert!(lo.gear.iter().any(|&d| d != 0));
        // Saved and read back the same.
        assert_eq!(Account::from_json(&acc.to_json()), Some(acc));
    }
}
