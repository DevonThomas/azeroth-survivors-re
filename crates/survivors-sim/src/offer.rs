//! The level-up deal: three choices weighted toward what the hero already has.

use crate::data::{self, AbilityDef, PassiveDef};
use crate::{Owned, Rng, Run};

#[derive(Clone, Copy)]
pub enum Choice {
    NewAbility(&'static AbilityDef),
    Upgrade(usize),
    NewPassive(&'static PassiveDef),
    UpPassive(usize),
    Heal,
}

impl PartialEq for Choice {
    fn eq(&self, other: &Self) -> bool {
        self.encode() == other.encode()
    }
}

impl std::fmt::Debug for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Choice{:?}", self.encode())
    }
}

impl Choice {
    /// The wire form: a tag and an index into the hero's slots or the static tables.
    pub fn encode(self) -> (u8, u16) {
        match self {
            Self::NewAbility(def) => (0, ability_index(def) as u16),
            Self::Upgrade(k) => (1, k as u16),
            Self::NewPassive(p) => (2, passive_index(p) as u16),
            Self::UpPassive(k) => (3, k as u16),
            Self::Heal => (4, 0),
        }
    }

    pub fn decode((tag, i): (u8, u16)) -> Option<Self> {
        let i = usize::from(i);
        Some(match tag {
            0 => Self::NewAbility(data::ABILITIES.get(i)?),
            1 => Self::Upgrade(i),
            2 => Self::NewPassive(data::PASSIVES.get(i)?),
            3 => Self::UpPassive(i),
            4 => Self::Heal,
            _ => return None,
        })
    }

    /// Apply to `run`; `false` when it no longer fits (a stale pick).
    pub fn apply(self, run: &mut Run) -> bool {
        match self {
            Self::NewAbility(def) => {
                if run.abilities.iter().any(|o| o.def.key == def.key)
                    || run.abilities.len() >= data::MAX_ABILITIES
                {
                    return false;
                }
                run.abilities.push(Owned {
                    def,
                    level: 1,
                    cd: 0.3,
                });
            }
            Self::Upgrade(k) => match run.abilities.get_mut(k) {
                Some(o) if o.level < data::MAX_ABILITY_LEVEL => o.level += 1,
                _ => return false,
            },
            Self::NewPassive(p) => {
                if p.class != run.class.id
                    || run.passives.iter().any(|(q, _)| q.key == p.key)
                    || run.passives.len() >= data::MAX_PASSIVES
                {
                    return false;
                }
                run.passives.push((p, 1));
            }
            Self::UpPassive(k) => match run.passives.get_mut(k) {
                Some(p) if p.1 < data::MAX_PASSIVE_LEVEL => p.1 += 1,
                _ => return false,
            },
            Self::Heal => run.hp = run.stats().max_hp,
        }
        run.version += 1;
        true
    }
}

impl Choice {
    /// The card's spell or passive key, for a banish; `None` for a rank-up or the heal.
    pub fn key(self) -> Option<&'static str> {
        match self {
            Self::NewAbility(def) => Some(def.key),
            Self::NewPassive(p) => Some(p.key),
            _ => None,
        }
    }
}

pub fn ability_index(def: &AbilityDef) -> usize {
    data::ABILITIES
        .iter()
        .position(|a| a.key == def.key)
        .unwrap_or(0)
}

pub fn passive_index(def: &PassiveDef) -> usize {
    data::PASSIVES
        .iter()
        .position(|p| p.key == def.key)
        .unwrap_or(0)
}

/// Three choices for `run`'s next level (one, Heal, when everything is maxed).
pub fn choices_for(run: &Run, rng: &mut Rng) -> Vec<Choice> {
    let mut pool: Vec<(Choice, f32)> = Vec::new();
    for (i, o) in run.abilities.iter().enumerate() {
        if o.level < data::MAX_ABILITY_LEVEL {
            pool.push((Choice::Upgrade(i), 1.2));
        }
    }
    if run.abilities.len() < data::MAX_ABILITIES {
        for key in run.class.pool {
            if !run.abilities.iter().any(|o| o.def.key == *key) && !run.banned.contains(key) {
                pool.push((Choice::NewAbility(data::ability(key)), 1.0));
            }
        }
    }
    for (i, (_, level)) in run.passives.iter().enumerate() {
        if *level < data::MAX_PASSIVE_LEVEL {
            pool.push((Choice::UpPassive(i), 0.8));
        }
    }
    if run.passives.len() < data::MAX_PASSIVES {
        for p in data::PASSIVES.iter().filter(|p| p.class == run.class.id) {
            if !run.passives.iter().any(|(q, _)| q.key == p.key) && !run.banned.contains(&p.key) {
                pool.push((Choice::NewPassive(p), 0.5));
            }
        }
    }
    let mut out = Vec::with_capacity(3);
    while out.len() < 3 && !pool.is_empty() {
        let total: f32 = pool.iter().map(|(_, w)| w).sum();
        let mut pick = rng.f32() * total;
        let mut idx = pool.len() - 1;
        for (i, (_, w)) in pool.iter().enumerate() {
            if pick < *w {
                idx = i;
                break;
            }
            pick -= w;
        }
        out.push(pool.swap_remove(idx).0);
    }
    if out.is_empty() {
        out.push(Choice::Heal);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hero is only ever dealt its own class's spells and passives, at every level of a long
    /// run, so a mage never sees a paladin's blessing.
    #[test]
    fn every_card_is_the_heros_own_class() {
        for (ci, class) in data::CLASSES.iter().enumerate() {
            let mut rng = Rng::seeded(ci as u64 + 1);
            let mut run = Run::start(class, 1);
            for _ in 0..60 {
                let offer = choices_for(&run, &mut rng);
                for c in &offer {
                    match *c {
                        Choice::NewAbility(def) => assert!(
                            class.pool.contains(&def.key),
                            "{} offered {}",
                            class.name,
                            def.key
                        ),
                        Choice::NewPassive(p) => assert_eq!(
                            p.class, class.id,
                            "{} offered {}",
                            class.name, p.name
                        ),
                        _ => {}
                    }
                }
                offer[0].apply(&mut run);
            }
            assert!(run.passives.iter().all(|(p, _)| p.class == class.id));
        }
    }

    #[test]
    fn passive_keys_are_unique() {
        let mut keys: Vec<&str> = data::PASSIVES.iter().map(|p| p.key).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "the wire names a passive by its key's index");
    }
}
