//! One hero's run: the class, the spells and passives with their ranks, health and the timed
//! buffs. The client's HUD reads its own hero's copy of this, mirrored from the [`crate::Sim`].

use crate::data::{self, AbilityDef, ClassDef, PassiveDef, Stat};

/// An ability the hero has, with its own cooldown.
#[derive(Clone)]
pub struct Owned {
    pub def: &'static AbilityDef,
    pub level: u8,
    pub cd: f32,
}

impl Owned {
    pub fn damage_mult(&self) -> f32 {
        1.0 + 0.3 * f32::from(self.level - 1)
    }
    pub fn cooldown(&self, stats: &Stats) -> f32 {
        let base = self.def.cooldown * 0.93f32.powi(i32::from(self.level) - 1);
        (base * (1.0 - stats.haste).max(0.35)).max(0.2)
    }
    pub fn count(&self) -> u32 {
        let extra = [3u8, 5, 7].iter().filter(|&&l| self.level >= l).count() as u32;
        self.def.count + extra
    }
    pub fn radius(&self, stats: &Stats) -> f32 {
        self.def.radius * (1.0 + 0.08 * f32::from(self.level - 1)) * stats.area
    }
    pub fn duration(&self) -> f32 {
        self.def.duration * (1.0 + 0.1 * f32::from(self.level - 1))
    }
}

/// The hero's derived numbers, from the base and the passives.
#[derive(Clone, Copy)]
pub struct Stats {
    pub might: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub regen: f32,
    pub haste: f32,
    pub area: f32,
    pub magnet: f32,
    pub growth: f32,
    pub armor: f32,
    pub crit: f32,
}

pub const BASE_HP: f32 = 120.0;
pub const BASE_SPEED: f32 = 7.0;
pub const BASE_MAGNET: f32 = 4.5;

pub const LEARN_LEVEL: u32 = 5;

/// The mobility's run state: cooldown left, and the haste or vanish still running.
#[derive(Default, Clone, Copy)]
pub struct MobilityState {
    pub cd: f32,
    pub haste: f32,
    pub haste_t: f32,
    pub vanish_t: f32,
}

/// One hero's run state. The run-wide numbers (the clock, the bosses, the banner) are copied in
/// from the sim, so a HUD reading one `Run` has the whole picture.
#[derive(Clone)]
pub struct Run {
    pub class: &'static ClassDef,
    pub race: u8,
    pub elapsed: f32,
    pub kills: u32,
    pub level: u32,
    pub xp: f32,
    pub hp: f32,
    pub abilities: Vec<Owned>,
    pub passives: Vec<(&'static PassiveDef, u8)>,
    pub shield: f32,
    pub shield_t: f32,
    pub immune_t: f32,
    pub thorns: f32,
    pub thorns_t: f32,
    pub thorns_spell: u32,
    pub heal_rate: f32,
    pub heal_t: f32,
    /// The class mobility's cooldown and running effect.
    pub mobile: MobilityState,
    pub pending_levels: u32,
    pub bosses: u32,
    pub rings: u32,
    pub won: bool,
    /// Seconds since the hero fell. In co-op a fallen hero waits here for a revive.
    pub dead_t: Option<f32>,
    /// A teammate's revive under way, 0..1.
    pub revive: f32,
    pub next_guid: u64,
    pub damage_done: f64,
    /// Bumped whenever abilities or passives change, so the HUD rebuilds the icon rows once.
    pub version: u32,
    /// A centre-screen announcement and its seconds left.
    pub banner: Option<(String, f32)>,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            class: &data::CLASSES[0],
            race: 0,
            elapsed: 0.0,
            kills: 0,
            level: 1,
            xp: 0.0,
            hp: BASE_HP,
            abilities: Vec::new(),
            passives: Vec::new(),
            shield: 0.0,
            shield_t: 0.0,
            immune_t: 0.0,
            thorns: 0.0,
            thorns_t: 0.0,
            thorns_spell: 0,
            heal_rate: 0.0,
            heal_t: 0.0,
            mobile: MobilityState::default(),
            pending_levels: 0,
            bosses: 0,
            rings: 0,
            won: false,
            dead_t: None,
            revive: 0.0,
            next_guid: 1,
            damage_done: 0.0,
            version: 0,
            banner: None,
        }
    }
}

impl Run {
    /// A fresh run for `class`, with its starting spell.
    pub fn start(class: &'static ClassDef, race: u8) -> Self {
        let mut run = Self {
            class,
            race,
            ..Self::default()
        };
        run.abilities.push(Owned {
            def: data::ability(class.start),
            level: 1,
            cd: 0.5,
        });
        run.hp = run.stats().max_hp;
        run.version += 1;
        run
    }

    pub fn stats(&self) -> Stats {
        let mut s = Stats {
            might: 1.0,
            max_hp: BASE_HP,
            speed: BASE_SPEED,
            regen: 0.0,
            haste: 0.0,
            area: 1.0,
            magnet: BASE_MAGNET,
            growth: 1.0,
            armor: 0.0,
            crit: 0.05,
        };
        // Tanky classes start tougher, cloth starts quicker.
        match self.class.id {
            1 | 2 => s.max_hp += 40.0,
            7 | 11 | 3 | 4 => s.max_hp += 15.0,
            _ => s.speed += 0.5,
        }
        for &(p, level) in &self.passives {
            let v = p.per_level * f32::from(level);
            match p.stat {
                Stat::Might => s.might += v,
                Stat::MaxHp => s.max_hp *= 1.0 + v,
                Stat::Speed => s.speed *= 1.0 + v,
                Stat::Regen => s.regen += v,
                Stat::Haste => s.haste += v,
                Stat::Area => s.area += v,
                Stat::Magnet => s.magnet *= 1.0 + v,
                Stat::Growth => s.growth += v,
                Stat::Armor => s.armor += v,
                Stat::Crit => s.crit += v,
            }
        }
        s
    }

    /// A fresh serial for a client-side guid.
    pub fn guid(&mut self) -> u64 {
        self.next_guid += 1;
        self.next_guid
    }

    /// The class's mobility once the hero is level enough.
    pub fn mobility(&self) -> Option<&'static Mobility> {
        (self.level >= LEARN_LEVEL)
            .then(|| mobility_for(self.class.id))
            .flatten()
    }

    /// The run speed's multiplier from a haste still running.
    pub fn haste_mult(&self) -> f32 {
        if self.mobile.haste_t > 0.0 {
            self.mobile.haste
        } else {
            1.0
        }
    }

    pub fn down(&self) -> bool {
        self.dead_t.is_some()
    }

    /// Damage from one blow, after immunity, armour and the shield; returns what got through.
    pub fn hurt(&mut self, raw: f32) -> f32 {
        if self.immune_t > 0.0 || self.dead_t.is_some() {
            return 0.0;
        }
        let mut dmg = (raw - self.stats().armor).max(1.0);
        if self.shield_t > 0.0 && self.shield > 0.0 {
            let absorbed = dmg.min(self.shield);
            self.shield -= absorbed;
            dmg -= absorbed;
        }
        self.hp -= dmg;
        dmg
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Move {
    /// Jump this many yards along the way the hero faces.
    Blink(f32),
    /// Rush at the nearest enemy within this many yards, stunning it; else lunge that far.
    Charge(f32),
    /// Run faster by this factor for `secs`.
    Haste(f32),
    /// Haste, and enemies lose the hero (they stop chasing) for `secs`.
    Vanish(f32),
    /// Haste, and every enemy within this many yards flees in fear.
    Scatter(f32),
}

pub struct Mobility {
    pub spell: u32,
    pub name: &'static str,
    pub kind: Move,
    pub cooldown: f32,
    pub secs: f32,
    pub desc: &'static str,
}

/// One per class, indexed by class id.
pub fn mobility_for(class: u8) -> Option<&'static Mobility> {
    MOBILITY.iter().find(|(c, _)| *c == class).map(|(_, m)| m)
}

static MOBILITY: &[(u8, Mobility)] = &[
    (
        1,
        Mobility {
            spell: 100,
            name: "Charge",
            kind: Move::Charge(25.0),
            cooldown: 8.0,
            secs: 1.5,
            desc: "Charge the nearest enemy within 25 yards, stunning it.",
        },
    ),
    (
        2,
        Mobility {
            spell: 1044,
            name: "Blessing of Freedom",
            kind: Move::Haste(1.6),
            cooldown: 15.0,
            secs: 5.0,
            desc: "Nothing can hold you: run 60% faster for 5 sec.",
        },
    ),
    (
        3,
        Mobility {
            spell: 5118,
            name: "Aspect of the Cheetah",
            kind: Move::Haste(1.7),
            cooldown: 14.0,
            secs: 5.0,
            desc: "Run 70% faster for 5 sec.",
        },
    ),
    (
        4,
        Mobility {
            spell: 2983,
            name: "Sprint",
            kind: Move::Haste(1.8),
            cooldown: 15.0,
            secs: 4.0,
            desc: "Run 80% faster for 4 sec.",
        },
    ),
    (
        5,
        Mobility {
            spell: 586,
            name: "Fade",
            kind: Move::Vanish(1.4),
            cooldown: 18.0,
            secs: 4.0,
            desc: "Fade from the horde's sight: enemies lose you and you run 40% faster for 4 sec.",
        },
    ),
    (
        7,
        Mobility {
            spell: 2645,
            name: "Ghost Wolf",
            kind: Move::Haste(1.6),
            cooldown: 15.0,
            secs: 6.0,
            desc: "Take the form of a ghost wolf, running 60% faster for 6 sec.",
        },
    ),
    (
        8,
        Mobility {
            spell: 1953,
            name: "Blink",
            kind: Move::Blink(18.0),
            cooldown: 10.0,
            secs: 0.0,
            desc: "Teleport 18 yards forward.",
        },
    ),
    (
        9,
        Mobility {
            spell: 5484,
            name: "Howl of Terror",
            kind: Move::Scatter(12.0),
            cooldown: 20.0,
            secs: 4.0,
            desc: "Howl, sending every enemy within 12 yards fleeing in terror for 4 sec.",
        },
    ),
    (
        11,
        Mobility {
            spell: 1850,
            name: "Dash",
            kind: Move::Haste(1.8),
            cooldown: 15.0,
            secs: 4.0,
            desc: "Run 80% faster for 4 sec.",
        },
    ),
];
