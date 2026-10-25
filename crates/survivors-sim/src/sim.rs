//! One run: the party, the horde, the spells in flight and the clock, advanced by [`Sim::step`].

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use crate::data::{self, AbilityDef, Effect, EnemyDef, Kind};
use crate::grid::Grid;
use crate::Loadout;
use crate::snapshot::{quantize, yaw_byte};
use crate::{
    choices_for, flat, yaw_toward, AreaSnap, Choice, EnemySnap, Event, GemLook, GemSnap, Id, Move,
    PlayerSnap, Rng, Run, Snapshot, TotemSnap, COLOR_HEAL, COLOR_HURT, COLOR_MELEE, COLOR_SPELL,
    FIRST_SPAWN_ID,
};

pub const MAX_PLAYERS: usize = 4;
/// A fallen hero gets up with a teammate this close…
pub const REVIVE_RADIUS: f32 = 3.0;
/// …for this long.
pub const REVIVE_SECS: f32 = 3.0;
/// A co-op party waits this long on a card before it is picked for the slow one.
const OFFER_TIMEOUT: f32 = 25.0;
/// Seconds of everyone down before the run is over.
const WIPE_SECS: f32 = 3.0;
/// Most gems on the ground at once; past it, a kill's worth joins an existing gem.
const MAX_GEMS: usize = 350;
/// Most enemies at once, whatever the party's size.
const MAX_ALIVE: usize = 380;
/// More impacts than this per cast only cost frames.
const MAX_VISUAL_HITS: usize = 12;
/// The body radius enemies stop at.
pub(crate) const HERO_RADIUS: f32 = 0.7;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SimPhase {
    Playing,
    /// Someone in the party has a card to pick; the field holds still.
    LevelUp,
    Over,
}

/// Where a client's hero is: the client moves its own hero over its terrain and reports it.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeroInput {
    pub pos: Vec3,
    pub yaw: f32,
    pub flags: u32,
    pub speed: f32,
    pub vy: f32,
    /// The last [`Event::Teleport`] this client applied: until it catches up, its reports are
    /// from before the jump and are ignored.
    pub seq: u32,
}

pub(crate) struct Player {
    id: Id,
    name: String,
    display: u32,
    sex: u8,
    run: Run,
    input: HeroInput,
    offer: Vec<Choice>,
    offer_t: f32,
    teleports: u32,
    buffs: Vec<u32>,
}

impl Player {
    fn pos(&self) -> Vec2 {
        flat(self.input.pos)
    }
}

struct Dot {
    dps: f32,
    left: f32,
    tick: f32,
}

struct Enemy {
    id: Id,
    kind: u16,
    def: EnemyDef,
    pos: Vec2,
    yaw: f32,
    hp: f32,
    max_hp: f32,
    damage: f32,
    radius: f32,
    boss: bool,
    attack_cd: f32,
    slow: f32,
    slow_t: f32,
    root_t: f32,
    stun_t: f32,
    fear_t: f32,
    weaken: f32,
    weaken_t: f32,
    dots: Vec<Dot>,
    dead: bool,
    dead_t: f32,
    moving: f32,
    marks: [(u32, f32); 8],
}

impl Enemy {
    fn mark(&mut self, spell: u32, secs: f32) {
        if let Some(slot) = self.marks.iter_mut().find(|s| s.0 == spell) {
            slot.1 = slot.1.max(secs);
        } else if let Some(slot) = self.marks.iter_mut().find(|s| s.0 == 0) {
            *slot = (spell, secs);
        }
    }
}

#[derive(Clone, Copy)]
enum GemKind {
    Xp(u32),
    Heal(f32),
    Gold(u32),
}

struct Gem {
    id: Id,
    kind: GemKind,
    pos: Vec2,
    /// The hero it flies to, by index.
    to: Option<usize>,
    speed: f32,
}

struct Area {
    id: Id,
    owner: usize,
    spell: u32,
    pos: Vec2,
    damage: f32,
    dot: f32,
    dot_dur: f32,
    radius: f32,
    left: f32,
    every: f32,
    next_tick: f32,
    effect: Effect,
    mark: u32,
    impact: u32,
    overhead: bool,
}

struct Totem {
    id: Id,
    owner: usize,
    display: u32,
    pos: Vec2,
    cast: u32,
    damage: f32,
    range: f32,
    radius: f32,
    left: f32,
    tick: f32,
    pulse: bool,
}

/// One blow landing on one enemy.
#[derive(Clone)]
struct Hit {
    target: Id,
    /// The hero it is from, by index: the damage and kills are theirs, and a drain heals them.
    source: usize,
    damage: f32,
    dot: f32,
    dot_dur: f32,
    effect: Effect,
    mark: u32,
    mark_dur: f32,
    /// Also hits everything this close to the target, for less.
    splash: f32,
    crit: bool,
    color: u32,
}

impl Hit {
    fn direct(target: Id, source: usize, damage: f32, crit: bool) -> Self {
        Self {
            target,
            source,
            damage,
            dot: 0.0,
            dot_dur: 0.0,
            effect: Effect::None,
            mark: 0,
            mark_dur: 0.0,
            splash: 0.0,
            crit,
            color: COLOR_MELEE,
        }
    }
}

#[derive(Clone, Copy)]
struct Foe {
    id: Id,
    pos: Vec2,
    hp: f32,
    dist: f32,
    radius: f32,
}

struct Loot {
    pos: Vec2,
    xp: u32,
    boss: bool,
}

pub struct Sim {
    pub map: usize,
    center: Vec2,
    leash: f32,
    phase: SimPhase,
    tick: u32,
    elapsed: f32,
    kills: u32,
    bosses: u32,
    rings: u32,
    won: bool,
    level: u32,
    xp: f32,
    wipe_t: f32,
    players: Vec<Player>,
    enemies: Vec<Enemy>,
    by_id: HashMap<Id, usize>,
    gems: Vec<Gem>,
    areas: Vec<Area>,
    totems: Vec<Totem>,
    pending: Vec<(f32, Hit)>,
    loot: Vec<Loot>,
    rng: Rng,
    next_id: Id,
    spawn_t: f32,
    banner: Option<(String, f32)>,
    events: Vec<Event>,
    grid: Grid,
    speeds: HashMap<u32, f32>,
}

impl Sim {
    pub fn new(map: usize, seed: u64) -> Self {
        let map = map.min(data::MAPS.len() - 1);
        let def = &data::MAPS[map];
        Self {
            map,
            center: Vec2::new(-def.center[1], -def.center[0]),
            leash: def.leash,
            phase: SimPhase::Playing,
            tick: 0,
            elapsed: 0.0,
            kills: 0,
            bosses: 0,
            rings: 0,
            won: false,
            level: 1,
            xp: 0.0,
            wipe_t: 0.0,
            players: Vec::new(),
            enemies: Vec::new(),
            by_id: HashMap::new(),
            gems: Vec::new(),
            areas: Vec::new(),
            totems: Vec::new(),
            pending: Vec::new(),
            loot: Vec::new(),
            rng: Rng::seeded(seed),
            next_id: FIRST_SPAWN_ID,
            spawn_t: 0.0,
            banner: None,
            events: Vec::new(),
            grid: Grid::default(),
            speeds: crate::MISSILE_SPEEDS.iter().copied().collect(),
        }
    }

    /// The battleground's centre on the ground plane.
    pub fn center(&self) -> Vec2 {
        self.center
    }

    /// A spell's missile speed (yd/s), from `Spell.dbc`; hits from a spell with none land at once.
    pub fn set_missile_speed(&mut self, spell: u32, speed: f32) {
        self.speeds.insert(spell, speed);
    }

    pub fn phase(&self) -> SimPhase {
        self.phase
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn player_ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.players.iter().map(|p| p.id)
    }

    /// A hero joins: at the start, or into a run under way, where they catch up on the party's
    /// levels card by card. `None` with the party full.
    pub fn add_player(
        &mut self,
        name: &str,
        class: usize,
        race: u8,
        sex: u8,
        display: u32,
    ) -> Option<Id> {
        self.add_player_with(name, class, race, sex, display, Loadout::default())
    }

    /// [`Sim::add_player`] with what the hero brings from the account: gear, talents, perks.
    pub fn add_player_with(
        &mut self,
        name: &str,
        class: usize,
        race: u8,
        sex: u8,
        display: u32,
        loadout: Loadout,
    ) -> Option<Id> {
        if self.players.len() >= MAX_PLAYERS || self.phase == SimPhase::Over {
            return None;
        }
        let id = (1..=MAX_PLAYERS as Id).find(|i| !self.players.iter().any(|p| p.id == *i))?;
        let class = &data::CLASSES[class.min(data::CLASSES.len() - 1)];
        let mut run = Run::start_with(class, race, loadout);
        run.level = self.level;
        run.pending_levels = self.level - 1;
        // Beside the party's first hero standing, or at the centre.
        let at = self
            .players
            .iter()
            .find(|p| !p.run.down())
            .map_or(self.center, |p| p.pos() + Vec2::new(1.5, 1.5));
        let y = self.players.first().map_or(0.0, |p| p.input.pos.y);
        self.players.push(Player {
            id,
            name: name.chars().take(16).collect(),
            display,
            sex,
            run,
            input: HeroInput {
                pos: Vec3::new(at.x, y, at.y),
                ..HeroInput::default()
            },
            offer: Vec::new(),
            offer_t: 0.0,
            teleports: 0,
            buffs: Vec::new(),
        });
        self.deal_offers();
        if self.players.len() > 1 {
            self.banner = Some((format!("{} joins the fight!", self.players[self.players.len() - 1].name), 3.0));
        }
        Some(id)
    }

    pub fn remove_player(&mut self, id: Id) {
        let Some(i) = self.players.iter().position(|p| p.id == id) else {
            return;
        };
        let gone = self.players.remove(i);
        // Indices past the leaver shift down; anything owned by the leaver goes with them.
        for g in &mut self.gems {
            g.to = match g.to {
                Some(t) if t == i => None,
                Some(t) if t > i => Some(t - 1),
                t => t,
            };
        }
        self.areas.retain(|a| a.owner != i);
        self.totems.retain(|t| t.owner != i);
        self.pending.retain(|(_, h)| h.source != i);
        for a in &mut self.areas {
            if a.owner > i {
                a.owner -= 1;
            }
        }
        for t in &mut self.totems {
            if t.owner > i {
                t.owner -= 1;
            }
        }
        for (_, h) in &mut self.pending {
            if h.source > i {
                h.source -= 1;
            }
        }
        if !self.players.is_empty() {
            self.banner = Some((format!("{} has left.", gone.name), 3.0));
        }
        self.update_phase();
    }

    pub fn set_input(&mut self, id: Id, input: HeroInput) {
        let center = self.center;
        let leash = self.leash;
        let Some(p) = self.players.iter_mut().find(|p| p.id == id) else {
            return;
        };
        if input.seq < p.teleports || p.run.down() {
            return;
        }
        let mut input = input;
        if !input.pos.is_finite() {
            return;
        }
        // Inside the battleground's leash, with a little slack for the client's own clamp.
        let off = flat(input.pos) - center;
        if off.length() > leash + 2.0 {
            let held = center + off.normalize() * leash;
            input.pos.x = held.x;
            input.pos.z = held.y;
        }
        p.input = input;
    }

    /// The hero's own state, with the run-wide numbers filled in, for its HUD.
    pub fn run_of(&self, id: Id) -> Option<Run> {
        let p = self.players.iter().find(|p| p.id == id)?;
        let mut run = p.run.clone();
        run.elapsed = self.elapsed;
        run.level = self.level;
        run.xp = self.xp;
        run.xp_next = self.xp_next();
        run.bosses = self.bosses;
        run.rings = self.rings;
        run.won = self.won;
        run.banner = self.banner.clone();
        Some(run)
    }

    pub fn offer_of(&self, id: Id) -> &[Choice] {
        self.players
            .iter()
            .find(|p| p.id == id)
            .map_or(&[], |p| &p.offer[..])
    }

    /// The experience the party needs for its next level: more heroes kill more, so need more.
    pub fn xp_next(&self) -> f32 {
        data::xp_to_next(self.level) * self.crowd()
    }

    /// How many more enemies a party of this size faces than a lone hero.
    fn crowd(&self) -> f32 {
        1.0 + 0.5 * (self.players.len().max(1) - 1) as f32
    }

    /// Take card `index` of `id`'s offer.
    pub fn choose(&mut self, id: Id, index: usize) {
        let Some(pi) = self.players.iter().position(|p| p.id == id) else {
            return;
        };
        self.pick(pi, index);
    }

    /// Deal `id` a fresh hand, if a reroll is left.
    pub fn reroll(&mut self, id: Id) {
        let Some(p) = self.players.iter_mut().find(|p| p.id == id) else {
            return;
        };
        if p.offer.is_empty() || p.run.rerolls == 0 {
            return;
        }
        p.run.rerolls -= 1;
        p.offer = choices_for(&p.run, &mut self.rng);
        p.offer_t = 0.0;
    }

    /// Never deal card `index`'s spell or passive again this run, and deal a fresh hand.
    pub fn banish(&mut self, id: Id, index: usize) {
        let Some(p) = self.players.iter_mut().find(|p| p.id == id) else {
            return;
        };
        let Some(key) = p.offer.get(index).and_then(|c| c.key()) else {
            return;
        };
        if p.run.banishes == 0 {
            return;
        }
        p.run.banishes -= 1;
        p.run.banned.push(key);
        p.offer = choices_for(&p.run, &mut self.rng);
        p.offer_t = 0.0;
    }

    /// Pass on the hand for gold, where the loadout allows it.
    pub fn skip(&mut self, id: Id) {
        let Some(pi) = self.players.iter().position(|p| p.id == id) else {
            return;
        };
        let level = self.level;
        let p = &mut self.players[pi];
        if p.offer.is_empty() || !p.run.loadout.skips {
            return;
        }
        p.run.gold += 10 + 3 * level;
        p.run.pending_levels = p.run.pending_levels.saturating_sub(1);
        p.offer.clear();
        if p.run.pending_levels > 0 {
            p.offer = choices_for(&p.run, &mut self.rng);
            p.offer_t = 0.0;
        }
        self.update_phase();
    }

    fn pick(&mut self, pi: usize, index: usize) {
        let p = &mut self.players[pi];
        let Some(&choice) = p.offer.get(index) else {
            return;
        };
        choice.apply(&mut p.run);
        p.run.pending_levels = p.run.pending_levels.saturating_sub(1);
        p.offer.clear();
        if p.run.pending_levels > 0 {
            p.offer = choices_for(&p.run, &mut self.rng);
            p.offer_t = 0.0;
        }
        self.update_phase();
    }

    fn deal_offers(&mut self) {
        for p in &mut self.players {
            if p.run.pending_levels > 0 && p.offer.is_empty() {
                p.offer = choices_for(&p.run, &mut self.rng);
                p.offer_t = 0.0;
            }
        }
        self.update_phase();
    }

    fn update_phase(&mut self) {
        if self.phase == SimPhase::Over {
            return;
        }
        self.phase = if self.players.iter().any(|p| !p.offer.is_empty()) {
            SimPhase::LevelUp
        } else {
            SimPhase::Playing
        };
    }

    /// The class's movement spell, aimed at `aim` (the cursor on the ground) where there is one.
    pub fn cast_mobility(&mut self, id: Id, aim: Option<Vec2>) {
        if self.phase != SimPhase::Playing {
            return;
        }
        let Some(pi) = self.players.iter().position(|p| p.id == id) else {
            return;
        };
        let (center, leash) = (self.center, self.leash);
        let p = &mut self.players[pi];
        if p.run.down() || p.run.mobile.cd > 0.0 {
            return;
        }
        let mut run = p.run.clone();
        run.level = self.level;
        let Some(def) = run.mobility() else { return };
        let pos = p.pos();
        let facing = Vec2::new(-p.input.yaw.sin(), -p.input.yaw.cos());
        let cursor = aim.filter(|a| (*a - pos).length() > 0.5);
        let toward = cursor.map_or(facing, |a| (a - pos).normalize_or(facing));
        let held = |q: Vec2| {
            let off = q - center;
            if off.length() > leash {
                center + off.normalize() * leash
            } else {
                q
            }
        };
        let mut jump = None;
        match def.kind {
            Move::Blink(yd) => {
                self.events.push(Event::Cast {
                    caster: id,
                    spell: def.spell,
                    targets: vec![id],
                    dest: None,
                });
                let far = cursor.map_or(yd, |a| (a - pos).length().min(yd));
                jump = Some((held(pos + toward * far), yaw_toward(toward)));
            }
            Move::Charge(yd) => {
                // Unaimed, the foe nearest the ground ahead of the hero.
                let mark = cursor.unwrap_or(pos + facing * yd * 0.5);
                let target = self
                    .enemies
                    .iter_mut()
                    .filter(|e| !e.dead && (e.pos - pos).length() <= yd)
                    .min_by(|a, b| (a.pos - mark).length().total_cmp(&(b.pos - mark).length()));
                match target {
                    Some(e) => {
                        e.stun_t = e.stun_t.max(def.secs);
                        let dir = (e.pos - pos).normalize_or(facing);
                        self.events.push(Event::Cast {
                            caster: id,
                            spell: def.spell,
                            targets: vec![e.id],
                            dest: None,
                        });
                        jump = Some((held(e.pos - dir * 1.5), yaw_toward(dir)));
                    }
                    None => {
                        self.events.push(Event::Cast {
                            caster: id,
                            spell: def.spell,
                            targets: vec![id],
                            dest: None,
                        });
                        jump = Some((held(pos + toward * yd * 0.5), yaw_toward(toward)));
                    }
                }
            }
            Move::Haste(x) | Move::Vanish(x) => {
                self.events.push(Event::Cast {
                    caster: id,
                    spell: def.spell,
                    targets: vec![id],
                    dest: None,
                });
                p.run.mobile.haste = x;
                p.run.mobile.haste_t = def.secs;
                if matches!(def.kind, Move::Vanish(_)) {
                    p.run.mobile.vanish_t = def.secs;
                }
            }
            Move::Scatter(r) => {
                self.events.push(Event::Cast {
                    caster: id,
                    spell: def.spell,
                    targets: vec![id],
                    dest: None,
                });
                for e in &mut self.enemies {
                    if !e.dead && !e.boss && (e.pos - pos).length() <= r {
                        e.fear_t = e.fear_t.max(def.secs);
                    }
                }
            }
        }
        let p = &mut self.players[pi];
        p.run.mobile.cd = def.cooldown;
        if let Some((to, yaw)) = jump {
            p.teleports += 1;
            p.input.pos.x = to.x;
            p.input.pos.z = to.y;
            p.input.yaw = yaw;
            self.events.push(Event::Teleport {
                player: id,
                to,
                yaw,
                seq: p.teleports,
            });
        }
    }

    /// The events since the last drain.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn new_id(&mut self) -> Id {
        self.next_id = self.next_id.wrapping_add(1).max(FIRST_SPAWN_ID);
        self.next_id
    }

    fn speed_of(&self, spell: u32) -> f32 {
        self.speeds.get(&spell).copied().unwrap_or(0.0)
    }

    /// Advance the run by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        if dt <= 0.0 || self.players.is_empty() {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        match self.phase {
            SimPhase::Over => return,
            SimPhase::LevelUp => {
                // A party waits on its slowest picker only so long; a lone hero takes all day.
                if self.players.len() > 1 {
                    let late: Vec<usize> = self
                        .players
                        .iter_mut()
                        .enumerate()
                        .filter(|(_, p)| !p.offer.is_empty())
                        .filter_map(|(i, p)| {
                            p.offer_t += dt;
                            (p.offer_t >= OFFER_TIMEOUT).then_some(i)
                        })
                        .collect();
                    for i in late {
                        let n = self.players[i].offer.len();
                        let pick = self.rng.index(n);
                        self.pick(i, pick);
                    }
                }
                return;
            }
            SimPhase::Playing => {}
        }
        self.elapsed += dt;
        if let Some((_, t)) = &mut self.banner {
            *t -= dt;
            if *t <= 0.0 {
                self.banner = None;
            }
        }
        self.hero_vitals(dt);
        if self.phase == SimPhase::Over {
            return;
        }
        self.spawn_enemies(dt);
        self.reindex();
        self.move_enemies(dt);
        for pi in 0..self.players.len() {
            self.cast_abilities(pi, dt);
        }
        self.tick_areas(dt);
        self.apply_hits(dt);
        self.reap(dt);
        self.drop_loot();
        self.collect_gems(dt);
    }

    fn reindex(&mut self) {
        self.by_id.clear();
        for (i, e) in self.enemies.iter().enumerate() {
            self.by_id.insert(e.id, i);
        }
        self.grid
            .rebuild(self.enemies.iter().map(|e| (!e.dead).then_some(e.pos)));
    }

    /// Regeneration, the timed buffs, falling, revives, the wipe and the bell.
    fn hero_vitals(&mut self, dt: f32) {
        let alive: Vec<(Id, Vec2)> = self
            .players
            .iter()
            .filter(|p| !p.run.down())
            .map(|p| (p.id, p.pos()))
            .collect();
        for p in &mut self.players {
            let stats = p.run.stats();
            let r = &mut p.run;
            let m = &mut r.mobile;
            m.cd = (m.cd - dt).max(0.0);
            m.haste_t = (m.haste_t - dt).max(0.0);
            m.vanish_t = (m.vanish_t - dt).max(0.0);
            if let Some(t) = r.dead_t.as_mut() {
                *t += dt;
                // A teammate standing over the fallen brings them back.
                let pos = flat(p.input.pos);
                let helper = alive
                    .iter()
                    .find(|(_, q)| (*q - pos).length() <= REVIVE_RADIUS)
                    .map(|(id, _)| *id);
                match helper {
                    Some(by) => {
                        r.revive += dt / REVIVE_SECS;
                        if r.revive >= 1.0 {
                            r.revive = 0.0;
                            r.dead_t = None;
                            r.hp = stats.max_hp * 0.5;
                            r.immune_t = 2.0;
                            self.events.push(Event::Revived { player: p.id, by });
                        }
                    }
                    None => r.revive = (r.revive - dt / REVIVE_SECS).max(0.0),
                }
                p.buffs.clear();
                continue;
            }
            for t in [&mut r.shield_t, &mut r.immune_t, &mut r.thorns_t] {
                *t = (*t - dt).max(0.0);
            }
            let mut heal = stats.regen * dt;
            if r.heal_t > 0.0 {
                heal += r.heal_rate * dt.min(r.heal_t);
                r.heal_t = (r.heal_t - dt).max(0.0);
            }
            r.hp = (r.hp + heal).min(stats.max_hp);

            // The buffs show as their own state kits: the shield bubble, the immunity glow, the orbs.
            p.buffs.clear();
            if r.shield_t > 0.0 && r.shield > 0.0 {
                p.buffs.push(17);
            }
            if r.immune_t > 0.0 {
                p.buffs.push(642);
            }
            if r.thorns_t > 0.0 {
                p.buffs.push(r.thorns_spell);
            }
            if r.heal_t > 0.0 {
                p.buffs.push(774);
            }
            for owned in &r.abilities {
                if owned.def.key == "lightning_shield" {
                    p.buffs.push(owned.def.spell);
                }
            }
            if r.passives.iter().any(|(q, _)| q.key == "renew") {
                p.buffs.push(139);
            }
            if r.hp <= 0.0 {
                r.hp = 0.0;
                r.dead_t = Some(0.0);
                r.revive = 0.0;
                p.buffs.clear();
                self.events.push(Event::Downed { player: p.id });
            }
        }
        if self.players.iter().all(|p| p.run.down()) {
            self.wipe_t += dt;
            if self.wipe_t >= WIPE_SECS {
                self.phase = SimPhase::Over;
            }
        } else {
            self.wipe_t = 0.0;
            if self.elapsed >= data::VICTORY_SECS {
                self.won = true;
                // A last flourish before the results.
                for p in &self.players {
                    self.events.push(Event::Cast {
                        caster: p.id,
                        spell: 642,
                        targets: vec![p.id],
                        dest: None,
                    });
                }
                self.phase = SimPhase::Over;
            }
        }
    }

    fn spawn_enemy(&mut self, roster_i: usize, boss: bool, pos: Vec2, toward: Vec2) {
        let roster = data::roster(self.map);
        let def = if boss {
            roster.bosses[roster_i]
        } else {
            roster.enemies[roster_i]
        };
        let n = self.players.len().max(1) as f32;
        let mut hp = def.hp * data::hp_scale(self.elapsed);
        // A boss stands up to the whole party.
        if boss {
            hp *= 1.0 + 0.75 * (n - 1.0);
        }
        let id = self.new_id();
        self.enemies.push(Enemy {
            id,
            kind: roster_i as u16 | if boss { EnemySnap::BOSS } else { 0 },
            def,
            pos,
            yaw: yaw_toward(toward - pos),
            hp,
            max_hp: hp,
            damage: def.damage * data::damage_scale(self.elapsed),
            radius: (if def.big { 1.3 } else { 0.6 }) * def.scale,
            boss,
            attack_cd: 0.5,
            slow: 1.0,
            slow_t: 0.0,
            root_t: 0.0,
            stun_t: 0.0,
            fear_t: 0.0,
            weaken: 1.0,
            weaken_t: 0.0,
            dots: Vec::new(),
            dead: false,
            dead_t: 0.0,
            moving: 0.0,
            marks: [(0, 0.0); 8],
        });
    }

    /// Keep the horde topped up on a ring outside the view of a hero, with the bosses and the
    /// encircling waves on the clock.
    fn spawn_enemies(&mut self, dt: f32) {
        let standing: Vec<Vec2> = self
            .players
            .iter()
            .filter(|p| !p.run.down())
            .map(Player::pos)
            .collect();
        if standing.is_empty() {
            return;
        }
        let elapsed = self.elapsed;
        let roster = data::roster(self.map);
        let pool = data::wave_pool(elapsed, roster.enemies.len());
        let crowd = self.crowd();

        let due_bosses = ((elapsed / data::BOSS_EVERY_SECS) as u32).min(roster.bosses.len() as u32);
        while self.bosses < due_bosses {
            let at = standing[self.rng.index(standing.len())];
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let pos = at + Vec2::new(a.cos(), a.sin()) * 30.0;
            let b = self.bosses as usize;
            self.spawn_enemy(b, true, pos, at);
            self.bosses += 1;
            self.banner = Some((format!("{} approaches!", roster.bosses[b].name), 4.0));
        }

        // Half way between bosses, a closing ring of the moment's weakest around every hero.
        let due_rings = ((elapsed + data::BOSS_EVERY_SECS / 2.0) / data::BOSS_EVERY_SECS) as u32;
        if self.rings < due_rings {
            self.rings = due_rings;
            let n = 28 + (data::progress(elapsed) * 60.0) as usize;
            let n = (n as f32 / standing.len() as f32 * crowd) as usize;
            for &at in &standing {
                for i in 0..n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    self.spawn_enemy(pool[0], false, at + Vec2::new(a.cos(), a.sin()) * 26.0, at);
                }
            }
            self.banner = Some(("They're surrounding you!".into(), 3.0));
        }

        self.spawn_t -= dt;
        if self.spawn_t > 0.0 {
            return;
        }
        self.spawn_t = 0.12;
        let alive = self.enemies.iter().filter(|e| !e.dead).count();
        let target = ((data::target_alive(elapsed) as f32 * crowd) as usize).min(MAX_ALIVE);
        if alive >= target {
            return;
        }
        let n = (((target - alive) as f32) / 6.0).ceil().min(4.0 * crowd) as usize;
        for _ in 0..n {
            let at = standing[self.rng.index(standing.len())];
            let kind = pool[self.rng.index(pool.len())];
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let r = self.rng.range(34.0, 42.0);
            self.spawn_enemy(kind, false, at + Vec2::new(a.cos(), a.sin()) * r, at);
        }
    }

    /// The chase: each at the nearest hero it can see, pushed apart from each other, stopped by
    /// crowd control, and swinging when in reach.
    fn move_enemies(&mut self, dt: f32) {
        // Heroes the horde can chase: standing and not faded.
        let targets: Vec<(usize, Vec2)> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.run.down() && p.run.mobile.vanish_t <= 0.0)
            .map(|(i, p)| (i, p.pos()))
            .collect();
        let anyone: Vec<Vec2> = self
            .players
            .iter()
            .filter(|p| !p.run.down())
            .map(Player::pos)
            .collect();

        // Separation over the grid: only neighbours in the same or adjacent cells push.
        let mut push = vec![Vec2::ZERO; self.enemies.len()];
        for (i, e) in self.enemies.iter().enumerate() {
            if e.dead {
                continue;
            }
            let mut acc = Vec2::ZERO;
            self.grid.near(e.pos, 3.0, |j| {
                if j == i {
                    return;
                }
                let o = &self.enemies[j];
                let d = e.pos - o.pos;
                let len = d.length();
                let min = e.radius + o.radius;
                if len < min && len > 1e-4 {
                    acc += d / len * (min - len);
                } else if len <= 1e-4 {
                    acc += Vec2::new((i as f32).sin(), (i as f32).cos()) * 0.1;
                }
            });
            push[i] = acc;
        }

        let mut blows: Vec<(usize, Id, f32, bool)> = Vec::new();
        for (i, e) in self.enemies.iter_mut().enumerate() {
            if e.dead {
                e.moving = 0.0;
                continue;
            }
            e.attack_cd -= dt;
            for t in [
                &mut e.slow_t,
                &mut e.root_t,
                &mut e.stun_t,
                &mut e.fear_t,
                &mut e.weaken_t,
            ] {
                *t = (*t - dt).max(0.0);
            }
            if e.slow_t <= 0.0 {
                e.slow = 1.0;
            }
            if e.weaken_t <= 0.0 {
                e.weaken = 1.0;
            }
            for m in &mut e.marks {
                if m.0 != 0 {
                    m.1 -= dt;
                    if m.1 <= 0.0 {
                        *m = (0, 0.0);
                    }
                }
            }

            // Strays far behind every hero are brought back in front of the nearest, as the
            // reference genre does.
            if let Some(near) = anyone
                .iter()
                .min_by(|a, b| (**a - e.pos).length().total_cmp(&(**b - e.pos).length()))
            {
                let to = *near - e.pos;
                if to.length() > 75.0 {
                    e.pos = *near + to.normalize_or_zero() * 38.0;
                    e.moving = 0.0;
                    continue;
                }
            }

            let chase = targets
                .iter()
                .map(|&(pi, q)| (pi, q, (q - e.pos).length()))
                .min_by(|a, b| a.2.total_cmp(&b.2));
            let held = e.stun_t > 0.0 || e.root_t > 0.0 || chase.is_none();
            let mut step = Vec2::ZERO;
            let mut speed = 0.0;
            if let Some((pi, q, dist)) = chase {
                let to_hero = q - e.pos;
                let contact = dist <= HERO_RADIUS + e.radius + 0.35;
                if !held {
                    let dir = if e.fear_t > 0.0 {
                        -to_hero.normalize_or_zero()
                    } else {
                        to_hero.normalize_or_zero()
                    };
                    if !contact || e.fear_t > 0.0 {
                        speed = e.def.speed * e.slow;
                        step = dir * speed;
                    }
                    if dir != Vec2::ZERO {
                        let want = yaw_toward(dir);
                        let mut d = (want - e.yaw).rem_euclid(std::f32::consts::TAU);
                        if d > std::f32::consts::PI {
                            d -= std::f32::consts::TAU;
                        }
                        e.yaw += d * (dt * 10.0).min(1.0);
                    }
                }
                if contact && e.stun_t <= 0.0 && e.fear_t <= 0.0 && e.attack_cd <= 0.0 {
                    e.attack_cd = if e.boss { 1.0 } else { 1.3 };
                    blows.push((pi, e.id, e.damage * e.weaken, e.boss));
                }
            }
            e.pos += step * dt + push[i] * (dt * 8.0).min(0.5);
            e.moving = speed;
        }

        for (pi, attacker, raw, _) in blows {
            let p = &mut self.players[pi];
            let taken = p.run.hurt(raw);
            self.events.push(Event::Swing {
                attacker,
                victim: p.id,
                damage: taken as u32,
            });
            if taken > 0.0 {
                self.events.push(Event::Number {
                    anchor: p.id,
                    amount: taken.round().max(1.0) as u32,
                    crit: false,
                    color: COLOR_HURT,
                });
            }
            if p.run.thorns_t > 0.0 {
                let dmg = p.run.thorns * p.run.stats().might;
                self.pending
                    .push((self.elapsed, Hit::direct(attacker, pi, dmg, false)));
            }
        }
    }

    /// The living foes in reach of `from`, nearest first.
    fn foes_from(&self, from: Vec2, reach: f32) -> Vec<Foe> {
        let mut foes = Vec::new();
        self.grid.near(from, reach + 2.0, |j| {
            let e = &self.enemies[j];
            let dist = (e.pos - from).length();
            if dist <= reach + e.radius {
                foes.push(Foe {
                    id: e.id,
                    pos: e.pos,
                    hp: e.hp,
                    dist,
                    radius: e.radius,
                });
            }
        });
        foes.sort_by(|a, b| a.dist.total_cmp(&b.dist));
        foes
    }

    fn cast(&mut self, caster: Id, spell: u32, targets: &[Foe], dest: Option<Vec2>) {
        self.events.push(Event::Cast {
            caster,
            spell,
            targets: targets.iter().take(MAX_VISUAL_HITS).map(|f| f.id).collect(),
            dest,
        });
    }

    fn schedule(&mut self, delay: f32, hit: Hit) {
        self.pending.push((self.elapsed + delay.max(0.0), hit));
    }

    fn roll(&mut self, crit: f32, dmg: f32) -> (f32, bool) {
        if self.rng.f32() < crit {
            (dmg * 2.0, true)
        } else {
            (dmg, false)
        }
    }

    /// Auto-cast: every owned ability fires on its own cooldown at the targets its kind picks.
    fn cast_abilities(&mut self, pi: usize, dt: f32) {
        if self.players[pi].run.down() {
            return;
        }
        let me = self.players[pi].id;
        let hpos = self.players[pi].pos();
        let stats = self.players[pi].run.stats();
        // Every kind's reach is within this; the grid walk stays local.
        let foes = self.foes_from(hpos, 40.0);
        for i in 0..self.players[pi].run.abilities.len() {
            let o = &mut self.players[pi].run.abilities[i];
            o.cd -= dt;
            if o.cd > 0.0 {
                continue;
            }
            let o = o.clone();
            let def = o.def;
            let mult = o.damage_mult() * stats.might * self.players[pi].run.spell_damage(def);
            let p = Params {
                dmg: def.damage * mult,
                dot: def.dot * mult,
                count: o.count() as usize,
                radius: o.radius(&stats),
                duration: o.duration(),
                cooldown: o.cooldown(&stats) * self.players[pi].run.spell_cooldown(def),
            };
            let speed = self.speed_of(def.spell);
            let delay = |f: &Foe| if speed > 0.0 { f.dist / speed } else { 0.0 };
            let fired = match def.kind {
                Kind::Bolt | Kind::Strike | Kind::Melee => {
                    let reach = if def.kind == Kind::Melee {
                        def.range + 0.5
                    } else {
                        def.range
                    };
                    let targets = if matches!(
                        def.key,
                        "pyroblast" | "aimed_shot" | "exorcism" | "starfire"
                    ) {
                        toughest(&foes, reach, p.count)
                    } else {
                        nearest(&foes, reach, p.count)
                    };
                    if targets.is_empty() {
                        false
                    } else {
                        if def.kind == Kind::Melee {
                            self.events.push(Event::Swing {
                                attacker: me,
                                victim: targets[0].id,
                                damage: p.dmg as u32,
                            });
                            for f in &targets {
                                self.events.push(Event::Impact {
                                    target: f.id,
                                    spell: def.spell,
                                });
                            }
                        } else {
                            self.cast(me, def.spell, &targets, None);
                        }
                        for f in &targets {
                            let (dmg, crit) = self.roll(stats.crit, p.dmg);
                            self.schedule(delay(f), hit_for(def, &p, f.id, pi, dmg, crit));
                        }
                        true
                    }
                }
                Kind::Nova | Kind::Cone => {
                    let mut targets = around(&foes, hpos, p.radius);
                    if def.kind == Kind::Cone {
                        if let Some(first) = targets.first().copied() {
                            let aim = (first.pos - hpos).normalize_or_zero();
                            targets.retain(|f| {
                                let d = f.pos - hpos;
                                d.length() < 2.0 || d.normalize_or_zero().dot(aim) >= 0.8
                            });
                        }
                    }
                    if targets.is_empty() {
                        false
                    } else {
                        self.cast(me, def.spell, &targets, None);
                        for f in &targets {
                            let (dmg, crit) = self.roll(stats.crit, p.dmg);
                            self.schedule(delay(f), hit_for(def, &p, f.id, pi, dmg, crit));
                        }
                        true
                    }
                }
                Kind::Ground | Kind::GroundSelf => {
                    let center = if def.kind == Kind::GroundSelf {
                        foes.first()
                            .filter(|f| f.dist <= p.radius + 4.0)
                            .map(|_| hpos)
                    } else {
                        densest(&foes, def.range, p.radius)
                    };
                    match center {
                        None => false,
                        Some(center) => {
                            self.cast(me, def.spell, &[], Some(center));
                            let burst = def.duration <= 0.0;
                            let id = self.new_id();
                            self.areas.push(Area {
                                id,
                                owner: pi,
                                spell: def.spell,
                                pos: center,
                                damage: p.dmg,
                                dot: p.dot,
                                dot_dur: if burst { 5.0 } else { 0.0 },
                                radius: p.radius,
                                left: if burst { 1.5 } else { p.duration },
                                every: if burst { f32::INFINITY } else { 1.0 },
                                next_tick: if burst { 0.0 } else { 0.4 },
                                effect: def.effect,
                                mark: def.mark,
                                impact: if def.overhead { def.spell } else { 0 },
                                overhead: def.overhead,
                            });
                            true
                        }
                    }
                }
                Kind::Chain => match nearest(&foes, def.range, 1).first().copied() {
                    None => false,
                    Some(first) => {
                        let mut chain = vec![first];
                        while chain.len() < p.count + 1 {
                            let last = chain[chain.len() - 1].pos;
                            let next = self
                                .foes_from(last, p.radius)
                                .into_iter()
                                .find(|f| !chain.iter().any(|c| c.id == f.id));
                            match next {
                                Some(f) => chain.push(f),
                                None => break,
                            }
                        }
                        self.cast(me, def.spell, &chain, None);
                        for (k, f) in chain.iter().enumerate() {
                            let (dmg, crit) =
                                self.roll(stats.crit, p.dmg * 0.85f32.powi(k as i32));
                            self.schedule(0.08 * k as f32, hit_for(def, &p, f.id, pi, dmg, crit));
                        }
                        true
                    }
                },
                Kind::Dot => {
                    let mut pool: Vec<Foe> = foes
                        .iter()
                        .filter(|f| f.dist <= def.range)
                        .take(p.count * 3)
                        .copied()
                        .collect();
                    if pool.is_empty() {
                        false
                    } else {
                        let mut targets = Vec::with_capacity(p.count);
                        while targets.len() < p.count && !pool.is_empty() {
                            let k = self.rng.index(pool.len());
                            targets.push(pool.swap_remove(k));
                        }
                        self.cast(me, def.spell, &targets, None);
                        for f in &targets {
                            self.schedule(delay(f), hit_for(def, &p, f.id, pi, 0.0, false));
                        }
                        true
                    }
                }
                Kind::Aura => {
                    let targets = around(&foes, hpos, p.radius);
                    if targets.is_empty() {
                        false
                    } else {
                        for f in targets.iter().take(6) {
                            self.events.push(Event::Impact {
                                target: f.id,
                                spell: def.spell,
                            });
                        }
                        for f in &targets {
                            let (dmg, crit) = self.roll(stats.crit, p.dmg);
                            self.schedule(0.0, hit_for(def, &p, f.id, pi, dmg, crit));
                        }
                        true
                    }
                }
                Kind::Orbit => {
                    let targets = nearest(&foes, p.radius, p.count);
                    if targets.is_empty() {
                        false
                    } else {
                        for f in &targets {
                            self.events.push(Event::Impact {
                                target: f.id,
                                spell: 403,
                            });
                            let (dmg, crit) = self.roll(stats.crit, p.dmg);
                            self.schedule(0.0, hit_for(def, &p, f.id, pi, dmg, crit));
                        }
                        true
                    }
                }
                Kind::Shield => {
                    let run = &mut self.players[pi].run;
                    run.shield = p.dmg;
                    run.shield_t = p.duration;
                    self.self_cast(me, def.spell);
                    true
                }
                Kind::Immune => {
                    if foes.first().is_some_and(|f| f.dist <= 5.0) {
                        self.players[pi].run.immune_t = p.duration;
                        self.self_cast(me, def.spell);
                        true
                    } else {
                        false
                    }
                }
                Kind::Thorns => {
                    if foes.first().is_some_and(|f| f.dist <= 8.0) {
                        let run = &mut self.players[pi].run;
                        run.thorns = p.dmg;
                        run.thorns_t = p.duration;
                        run.thorns_spell = def.spell;
                        self.self_cast(me, def.spell);
                        true
                    } else {
                        false
                    }
                }
                Kind::Heal => self.cast_heal(pi, def, &p),
                Kind::Totem => {
                    if foes
                        .first()
                        .is_some_and(|f| f.dist <= def.range.max(p.radius) + 6.0)
                    {
                        self.cast(me, def.spell, &[], None);
                        let a = self.rng.range(0.0, std::f32::consts::TAU);
                        let id = self.new_id();
                        self.totems.push(Totem {
                            id,
                            owner: pi,
                            display: def.totem.0,
                            pos: hpos + Vec2::new(a.cos(), a.sin()) * 2.0,
                            cast: def.totem.1,
                            damage: p.dmg,
                            range: def.range,
                            radius: p.radius,
                            left: p.duration,
                            tick: 0.3,
                            pulse: def.key == "magma_totem",
                        });
                        true
                    } else {
                        false
                    }
                }
            };
            self.players[pi].run.abilities[i].cd = if fired { p.cooldown } else { 0.2 };
        }
    }

    fn self_cast(&mut self, me: Id, spell: u32) {
        self.events.push(Event::Cast {
            caster: me,
            spell,
            targets: vec![me],
            dest: None,
        });
    }

    /// A heal-over-time on the most hurt hero in the party: in co-op the healers heal the group.
    fn cast_heal(&mut self, pi: usize, def: &AbilityDef, p: &Params) -> bool {
        let from = self.players[pi].pos();
        let hurt = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, q)| !q.run.down() && (q.pos() - from).length() <= 30.0)
            .map(|(i, q)| (i, q.run.hp / q.run.stats().max_hp))
            .filter(|&(_, frac)| frac < 0.85)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((ti, _)) = hurt else { return false };
        let me = self.players[pi].id;
        let target = self.players[ti].id;
        let run = &mut self.players[ti].run;
        run.heal_rate = p.dmg / p.duration.max(1.0);
        run.heal_t = p.duration;
        self.events.push(Event::Cast {
            caster: me,
            spell: def.spell,
            targets: vec![target],
            dest: None,
        });
        self.events.push(Event::Number {
            anchor: target,
            amount: p.dmg.round().max(1.0) as u32,
            crit: false,
            color: COLOR_HEAL,
        });
        true
    }

    /// Ground areas tick, and totems fight, until they run out.
    fn tick_areas(&mut self, dt: f32) {
        let mut areas = std::mem::take(&mut self.areas);
        for area in &mut areas {
            area.left -= dt;
            area.next_tick -= dt;
            if area.next_tick > 0.0 || area.owner >= self.players.len() {
                continue;
            }
            area.next_tick += area.every;
            let crit = self.players[area.owner].run.stats().crit;
            let hit = around(&self.foes_from(area.pos, area.radius), area.pos, area.radius);
            for (i, f) in hit.iter().enumerate() {
                if area.impact != 0 && i < MAX_VISUAL_HITS {
                    self.events.push(Event::Impact {
                        target: f.id,
                        spell: area.impact,
                    });
                }
                let (dmg, crit) = self.roll(crit, area.damage);
                self.schedule(
                    0.0,
                    Hit {
                        target: f.id,
                        source: area.owner,
                        damage: dmg,
                        dot: area.dot,
                        dot_dur: area.dot_dur,
                        effect: area.effect,
                        mark: area.mark,
                        mark_dur: 1.5,
                        splash: 0.0,
                        crit,
                        color: COLOR_SPELL,
                    },
                );
            }
        }
        areas.retain(|a| a.left > 0.0);
        self.areas = areas;

        let mut totems = std::mem::take(&mut self.totems);
        for totem in &mut totems {
            totem.left -= dt;
            totem.tick -= dt;
            if totem.left <= 0.0 || totem.tick > 0.0 || totem.owner >= self.players.len() {
                continue;
            }
            let crit = self.players[totem.owner].run.stats().crit;
            if totem.pulse {
                totem.tick = 2.0;
                let targets = around(&self.foes_from(totem.pos, totem.radius), totem.pos, totem.radius);
                if targets.is_empty() {
                    continue;
                }
                self.events.push(Event::Cast {
                    caster: totem.id,
                    spell: totem.cast,
                    targets: Vec::new(),
                    dest: None,
                });
                for f in &targets {
                    let (dmg, crit) = self.roll(crit, totem.damage);
                    self.schedule(
                        0.0,
                        Hit {
                            color: COLOR_SPELL,
                            ..Hit::direct(f.id, totem.owner, dmg, crit)
                        },
                    );
                }
            } else {
                totem.tick = 1.0;
                let Some(f) = self.foes_from(totem.pos, totem.range).first().copied() else {
                    continue;
                };
                self.events.push(Event::Cast {
                    caster: totem.id,
                    spell: totem.cast,
                    targets: vec![f.id],
                    dest: None,
                });
                let speed = self.speed_of(totem.cast);
                let delay = if speed > 0.0 { f.dist / speed } else { 0.0 };
                let (dmg, crit) = self.roll(crit, totem.damage);
                self.schedule(
                    delay,
                    Hit {
                        color: COLOR_SPELL,
                        ..Hit::direct(f.id, totem.owner, dmg, crit)
                    },
                );
            }
        }
        totems.retain(|t| t.left > 0.0);
        self.totems = totems;
    }

    /// Land every hit due: damage and its number, the over-time part, crowd control, the marks,
    /// and death with its loot.
    fn apply_hits(&mut self, dt: f32) {
        let now = self.elapsed;
        let mut due = Vec::new();
        self.pending.retain(|(t, h)| {
            if *t <= now {
                due.push(h.clone());
                false
            } else {
                true
            }
        });

        // Splash reaches the target's neighbours at seventy percent.
        let mut all = Vec::with_capacity(due.len());
        for h in due {
            if h.splash > 0.0 {
                if let Some(&ti) = self.by_id.get(&h.target) {
                    let c = self.enemies[ti].pos;
                    self.grid.near(c, h.splash, |j| {
                        let e = &self.enemies[j];
                        if e.id != h.target && !e.dead && (e.pos - c).length() <= h.splash {
                            all.push(Hit {
                                target: e.id,
                                damage: h.damage * 0.7,
                                splash: 0.0,
                                mark: 0,
                                ..h.clone()
                            });
                        }
                    });
                }
            }
            all.push(Hit { splash: 0.0, ..h });
        }

        for h in all {
            let Some(&ei) = self.by_id.get(&h.target) else {
                continue;
            };
            let src = h.source.min(self.players.len().saturating_sub(1));
            let e = &mut self.enemies[ei];
            if e.dead {
                continue;
            }
            let mut dmg = h.damage;
            if let Effect::Execute(threshold) = h.effect {
                if !e.boss && e.hp / e.max_hp <= threshold {
                    dmg = dmg.max(e.hp);
                }
            }
            if dmg > 0.0 {
                e.hp -= dmg;
                self.events.push(Event::Number {
                    anchor: e.id,
                    amount: dmg.round().max(1.0) as u32,
                    crit: h.crit,
                    color: h.color,
                });
                if let Some(p) = self.players.get_mut(src) {
                    p.run.damage_done += f64::from(dmg);
                    if let Effect::Drain(frac) = h.effect {
                        let max = p.run.stats().max_hp;
                        if !p.run.down() {
                            p.run.hp = (p.run.hp + dmg * frac).min(max);
                        }
                    }
                }
            }
            if h.dot > 0.0 && h.dot_dur > 0.0 {
                if e.dots.len() >= 6 {
                    e.dots.remove(0);
                }
                e.dots.push(Dot {
                    dps: h.dot,
                    left: h.dot_dur,
                    tick: 1.0,
                });
            }
            let boss = e.boss;
            match h.effect {
                Effect::Slow(m, t) => {
                    let m = if boss { m.max(0.75) } else { m };
                    e.slow = if e.slow_t > 0.0 { e.slow.min(m) } else { m };
                    e.slow_t = e.slow_t.max(t);
                }
                Effect::Root(t) => e.root_t = e.root_t.max(if boss { t * 0.3 } else { t }),
                Effect::Stun(t) => e.stun_t = e.stun_t.max(if boss { t * 0.3 } else { t }),
                Effect::Fear(t) if !boss => e.fear_t = e.fear_t.max(t),
                Effect::Weaken(m, t) => {
                    e.weaken = m;
                    e.weaken_t = t;
                }
                _ => {}
            }
            if h.mark != 0 {
                e.mark(h.mark, h.mark_dur);
            }
            if e.hp <= 0.0 {
                self.kill(ei, src);
            }
        }

        // The over-time ticks, once a second each.
        for ei in 0..self.enemies.len() {
            let e = &mut self.enemies[ei];
            if e.dead || e.dots.is_empty() {
                continue;
            }
            let mut dealt = 0.0;
            for d in &mut e.dots {
                d.left -= dt;
                d.tick -= dt;
                if d.tick <= 0.0 {
                    d.tick += 1.0;
                    dealt += d.dps;
                }
            }
            e.dots.retain(|d| d.left > 0.0);
            if dealt > 0.0 {
                e.hp -= dealt;
                let id = e.id;
                let dead = e.hp <= 0.0;
                self.events.push(Event::Number {
                    anchor: id,
                    amount: dealt.round().max(1.0) as u32,
                    crit: false,
                    color: COLOR_SPELL,
                });
                // Over-time damage is shared credit: the party's first hero keeps the tally.
                if let Some(p) = self.players.first_mut() {
                    p.run.damage_done += f64::from(dealt);
                }
                if dead {
                    self.kill(ei, 0);
                }
            }
        }
    }

    fn kill(&mut self, ei: usize, by: usize) {
        let e = &mut self.enemies[ei];
        e.dead = true;
        e.hp = 0.0;
        e.dots.clear();
        e.marks = [(0, 0.0); 8];
        e.moving = 0.0;
        self.loot.push(Loot {
            pos: e.pos,
            xp: e.def.xp,
            boss: e.boss,
        });
        self.kills += 1;
        if let Some(p) = self.players.get_mut(by) {
            p.run.kills += 1;
        }
        if e.boss {
            self.banner = Some((format!("{} is slain!", e.def.name), 3.0));
            // Each client rolls its own hero's drop from its own tables.
            self.events.push(Event::BossDown {
                boss: u32::from(e.kind & !EnemySnap::BOSS),
                map: self.map as u16,
            });
        }
    }

    /// Corpses go once their death has played and faded.
    fn reap(&mut self, dt: f32) {
        let before = self.enemies.len();
        for e in &mut self.enemies {
            if e.dead {
                e.dead_t += dt;
            }
        }
        // The death plays out, then the client fades the corpse as it leaves the snapshot.
        self.enemies.retain(|e| e.dead_t < 2.5);
        if self.enemies.len() != before {
            self.reindex();
        }
    }

    fn drop_loot(&mut self) {
        let loot = std::mem::take(&mut self.loot);
        for d in loot {
            if d.boss || self.rng.f32() < 0.015 {
                let id = self.new_id();
                self.gems.push(Gem {
                    id,
                    kind: GemKind::Heal(if d.boss { 1000.0 } else { 30.0 }),
                    pos: d.pos + Vec2::new(0.8, 0.0),
                    to: None,
                    speed: 6.0,
                });
            }
            // Coins: a boss spills a purse, the horde a coin now and then, richer as the run goes.
            let coins = if d.boss {
                40 + 20 * self.bosses
            } else if self.rng.f32() < 0.06 {
                1 + (data::progress(self.elapsed) * 4.0) as u32 + d.xp / 6
            } else {
                0
            };
            if coins > 0 {
                let id = self.new_id();
                self.gems.push(Gem {
                    id,
                    kind: GemKind::Gold(coins),
                    pos: d.pos + Vec2::new(-0.6, 0.4),
                    to: None,
                    speed: 6.0,
                });
            }
            if self.gems.len() >= MAX_GEMS && !d.boss {
                if let Some(g) = self.gems.iter_mut().find(|g| matches!(g.kind, GemKind::Xp(_))) {
                    if let GemKind::Xp(xp) = &mut g.kind {
                        *xp += d.xp;
                    }
                    continue;
                }
            }
            let id = self.new_id();
            self.gems.push(Gem {
                id,
                kind: GemKind::Xp(d.xp),
                pos: d.pos,
                to: None,
                speed: 6.0,
            });
        }
    }

    /// Gems in a hero's magnet fly to them. Experience is the party's; health is the catcher's.
    fn collect_gems(&mut self, dt: f32) {
        let heroes: Vec<(Vec2, f32, f32, bool)> = self
            .players
            .iter()
            .map(|p| {
                let s = p.run.stats();
                (p.pos(), s.magnet, s.growth, p.run.down())
            })
            .collect();
        let mut gained = 0.0;
        let mut heals: Vec<(usize, f32)> = Vec::new();
        let mut coins = 0u32;
        self.gems.retain_mut(|g| {
            if g.to.is_none_or(|t| t >= heroes.len() || heroes[t].3) {
                g.to = heroes
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| !h.3 && (h.0 - g.pos).length() <= h.1)
                    .min_by(|a, b| (a.1 .0 - g.pos).length().total_cmp(&(b.1 .0 - g.pos).length()))
                    .map(|(i, _)| i);
            }
            let Some(t) = g.to else { return true };
            let d = heroes[t].0 - g.pos;
            let dist = d.length();
            if dist < 0.9 {
                match g.kind {
                    GemKind::Xp(xp) => gained += xp as f32 * data::XP_RATE * heroes[t].2,
                    GemKind::Heal(hp) => heals.push((t, hp)),
                    GemKind::Gold(g) => coins += g,
                }
                return false;
            }
            g.speed += 30.0 * dt;
            g.pos += d / dist * (g.speed * dt).min(dist);
            true
        });
        // Gold is everyone's: each hero banks a picked-up coin's worth, with their own gold bonus.
        if coins > 0 {
            for p in &mut self.players {
                let bonus = 1.0 + p.run.loadout.stats.gold;
                p.run.gold += (coins as f32 * bonus).round() as u32;
            }
        }
        for (t, hp) in heals {
            let r = &mut self.players[t].run;
            r.hp = (r.hp + hp).min(r.stats().max_hp);
        }
        self.xp += gained;
        let mut levels = 0;
        while self.xp >= self.xp_next() {
            self.xp -= self.xp_next();
            self.level += 1;
            levels += 1;
            if self.level == crate::LEARN_LEVEL {
                let names: Vec<&str> = self
                    .players
                    .iter()
                    .filter_map(|p| crate::mobility_for(p.run.class.id))
                    .map(|m| m.name)
                    .collect();
                if names.len() == 1 {
                    self.banner = Some((format!("You have learned {}! Press F.", names[0]), 4.0));
                } else if !names.is_empty() {
                    self.banner = Some(("Movement spells learned! Press F.".into(), 4.0));
                }
            }
        }
        if levels > 0 {
            for p in &mut self.players {
                p.run.level = self.level;
                // A fallen hero still levels: they pick when they get up, or now.
                p.run.pending_levels += levels;
            }
            self.deal_offers();
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let origin = self.center;
        Snapshot {
            tick: self.tick,
            elapsed: self.elapsed,
            origin,
            map: self.map as u16,
            level: self.level,
            xp: self.xp,
            xp_next: self.xp_next(),
            kills: self.kills,
            bosses: self.bosses,
            phase: match self.phase {
                SimPhase::Playing => 0,
                SimPhase::LevelUp => 1,
                SimPhase::Over => 2,
            },
            won: self.won,
            players: self
                .players
                .iter()
                .map(|p| PlayerSnap {
                    id: p.id,
                    name: p.name.clone(),
                    class: p.run.class.id,
                    race: p.run.race,
                    sex: p.sex,
                    display: p.display,
                    pos: p.input.pos,
                    yaw: p.input.yaw,
                    flags: p.input.flags,
                    speed: p.input.speed,
                    vy: p.input.vy,
                    hp: p.run.hp,
                    max_hp: p.run.stats().max_hp,
                    level: self.level,
                    down: p.run.down(),
                    revive: p.run.revive,
                    buffs: p.buffs.clone(),
                    kills: p.run.kills,
                    damage: p.run.damage_done,
                    picking: !p.offer.is_empty(),
                    look: p.run.loadout.look,
                    gear: p.run.loadout.gear,
                    gold: p.run.gold,
                })
                .collect(),
            enemies: self
                .enemies
                .iter()
                .map(|e| EnemySnap {
                    id: e.id,
                    kind: e.kind,
                    pos: quantize(e.pos, origin),
                    yaw: yaw_byte(e.yaw),
                    speed: (e.moving * 16.0).round().clamp(0.0, 255.0) as u8,
                    hp: ((e.hp / e.max_hp).clamp(0.0, 1.0) * 255.0).round() as u8,
                    dead: e.dead,
                    marks: e.marks.iter().filter(|m| m.0 != 0).map(|m| m.0).collect(),
                })
                .collect(),
            gems: self
                .gems
                .iter()
                .map(|g| GemSnap {
                    id: g.id,
                    look: match g.kind {
                        GemKind::Heal(_) => GemLook::Heal,
                        GemKind::Gold(_) => GemLook::Gold,
                        GemKind::Xp(xp) if xp >= 20 => GemLook::Purple,
                        GemKind::Xp(xp) if xp >= 5 => GemLook::Green,
                        GemKind::Xp(_) => GemLook::Small,
                    },
                    pos: quantize(g.pos, origin),
                    to: g.to.and_then(|t| self.players.get(t)).map(|p| p.id),
                })
                .collect(),
            totems: self
                .totems
                .iter()
                .map(|t| TotemSnap {
                    id: t.id,
                    display: t.display,
                    pos: t.pos,
                })
                .collect(),
            areas: self
                .areas
                .iter()
                .map(|a| AreaSnap {
                    id: a.id,
                    spell: a.spell,
                    radius: a.radius,
                    pos: a.pos,
                    overhead: a.overhead,
                })
                .collect(),
        }
    }

    /// Hand a solo hero the whole class pool at `level`, for testing the crowd at its thickest.
    pub fn grant_all(&mut self, id: Id, level: u8) {
        if let Some(p) = self.players.iter_mut().find(|p| p.id == id) {
            for key in p.run.class.pool {
                if !p.run.abilities.iter().any(|o| o.def.key == *key) {
                    p.run.abilities.push(crate::Owned {
                        def: data::ability(key),
                        level,
                        cd: 0.3,
                    });
                } else if let Some(o) = p.run.abilities.iter_mut().find(|o| o.def.key == *key) {
                    o.level = o.level.max(level);
                }
            }
            p.run.version += 1;
        }
    }

    /// Jump the clock, for testing the late run.
    pub fn skip_to(&mut self, secs: f32) {
        self.elapsed = self.elapsed.max(secs);
        self.bosses = self.bosses.max((secs / data::BOSS_EVERY_SECS) as u32);
        self.rings = self
            .rings
            .max(((secs + data::BOSS_EVERY_SECS / 2.0) / data::BOSS_EVERY_SECS) as u32);
    }

    /// Living enemies, for tests and the auto-runner.
    pub fn alive(&self) -> usize {
        self.enemies.iter().filter(|e| !e.dead).count()
    }
}

/// The numbers one cast uses, from the ability's level and the hero's stats.
struct Params {
    dmg: f32,
    dot: f32,
    count: usize,
    radius: f32,
    duration: f32,
    cooldown: f32,
}

fn mark_secs(def: &AbilityDef, duration: f32) -> f32 {
    match def.effect {
        Effect::Slow(_, t) | Effect::Weaken(_, t) => t,
        Effect::Root(t) | Effect::Stun(t) | Effect::Fear(t) => t,
        _ if duration > 0.0 => duration,
        _ => 3.0,
    }
}

fn hit_for(def: &AbilityDef, p: &Params, target: Id, source: usize, damage: f32, crit: bool) -> Hit {
    Hit {
        target,
        source,
        damage,
        dot: p.dot,
        dot_dur: p.duration,
        effect: def.effect,
        mark: def.mark,
        mark_dur: mark_secs(def, p.duration),
        splash: if matches!(def.kind, Kind::Bolt | Kind::Strike) {
            p.radius
        } else {
            0.0
        },
        crit,
        color: if def.kind == Kind::Melee {
            COLOR_MELEE
        } else {
            COLOR_SPELL
        },
    }
}

fn nearest(foes: &[Foe], range: f32, n: usize) -> Vec<Foe> {
    foes.iter()
        .filter(|f| f.dist <= range + f.radius)
        .take(n)
        .copied()
        .collect()
}

fn toughest(foes: &[Foe], range: f32, n: usize) -> Vec<Foe> {
    let mut v: Vec<Foe> = foes
        .iter()
        .filter(|f| f.dist <= range + f.radius)
        .copied()
        .collect();
    v.sort_by(|a, b| b.hp.total_cmp(&a.hp));
    v.truncate(n);
    v
}

fn around(foes: &[Foe], center: Vec2, r: f32) -> Vec<Foe> {
    foes.iter()
        .filter(|f| (f.pos - center).length() <= r + f.radius * 0.5)
        .copied()
        .collect()
}

/// The point among the nearest enemies in range with the most others within `r` of it.
fn densest(foes: &[Foe], range: f32, r: f32) -> Option<Vec2> {
    let near: Vec<&Foe> = foes.iter().filter(|f| f.dist <= range).take(40).collect();
    near.iter()
        .map(|f| {
            (
                f.pos,
                near.iter().filter(|o| (o.pos - f.pos).length() <= r).count(),
            )
        })
        .max_by_key(|&(_, n)| n)
        .map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero_at(sim: &mut Sim, id: Id, p: Vec2) {
        sim.set_input(
            id,
            HeroInput {
                pos: Vec3::new(p.x, 0.0, p.y),
                ..HeroInput::default()
            },
        );
    }

    #[test]
    fn solo_run_fights_and_levels() {
        let mut sim = Sim::new(0, 7);
        let id = sim.add_player("Solo", 7, 2, 0, 0).unwrap();
        let c = sim.center();
        let mut levels = 0;
        for _ in 0..(60 * 30) {
            hero_at(&mut sim, id, c);
            sim.step(1.0 / 30.0);
            if sim.phase() == SimPhase::LevelUp {
                sim.choose(id, 0);
                levels += 1;
            }
        }
        assert!(sim.kills > 0, "the hero should kill something in a minute");
        assert!(levels > 0, "and level up");
        assert!(sim.alive() > 0);
    }

    #[test]
    fn party_shares_levels_and_revives() {
        let mut sim = Sim::new(0, 11);
        let a = sim.add_player("A", 0, 1, 0, 0).unwrap();
        let b = sim.add_player("B", 6, 2, 0, 0).unwrap();
        assert_ne!(a, b);
        let c = sim.center();
        sim.players[1].run.hp = 0.0;
        for _ in 0..(5 * 30) {
            hero_at(&mut sim, a, c);
            hero_at(&mut sim, b, c + Vec2::new(1.0, 0.0));
            sim.step(1.0 / 30.0);
            while sim.phase() == SimPhase::LevelUp {
                for id in [a, b] {
                    if !sim.offer_of(id).is_empty() {
                        sim.choose(id, 0);
                    }
                }
            }
        }
        assert!(!sim.players[1].run.down(), "standing beside them revives the fallen");
        assert_eq!(sim.run_of(a).unwrap().level, sim.run_of(b).unwrap().level);
    }

    #[test]
    fn snapshot_round_trips_positions() {
        let mut sim = Sim::new(3, 5);
        let id = sim.add_player("Q", 2, 1, 0, 0).unwrap();
        let c = sim.center();
        hero_at(&mut sim, id, c);
        for _ in 0..90 {
            sim.step(1.0 / 30.0);
        }
        let snap = sim.snapshot();
        for (e, s) in sim.enemies.iter().zip(&snap.enemies) {
            assert!((s.point(snap.origin) - e.pos).length() < 0.02);
        }
    }

    #[test]
    fn late_crowd_stays_bounded() {
        let mut sim = Sim::new(0, 3);
        let ids: Vec<Id> = (0..4)
            .map(|i| sim.add_player("P", i * 2, 1, 0, 0).unwrap())
            .collect();
        sim.skip_to(700.0);
        let c = sim.center();
        let start = std::time::Instant::now();
        for _ in 0..(20 * 30) {
            for (k, id) in ids.iter().enumerate() {
                hero_at(&mut sim, *id, c + Vec2::new(k as f32 * 3.0, 0.0));
                sim.grant_all(*id, 8);
            }
            sim.step(1.0 / 30.0);
            while sim.phase() == SimPhase::LevelUp {
                for id in &ids {
                    if !sim.offer_of(*id).is_empty() {
                        sim.choose(*id, 0);
                    }
                }
            }
            sim.drain_events();
        }
        assert!(sim.alive() <= MAX_ALIVE);
        // 600 steps of a four-hero party in the thickest crowd: well inside a 30 Hz budget.
        let per = start.elapsed().as_secs_f64() / 600.0;
        eprintln!("late four-hero step: {:.3} ms", per * 1000.0);
    }

    #[test]
    fn late_joiner_catches_up_and_leavers_leave_cleanly() {
        let mut sim = Sim::new(0, 21);
        // Tough enough to stand in the open for the whole test.
        let tank = Loadout {
            stats: crate::StatBonus {
                health: 2000.0,
                regen: 40.0,
                ..crate::StatBonus::default()
            },
            ..Loadout::default()
        };
        let a = sim.add_player_with("A", 7, 2, 0, 0, tank.clone()).unwrap();
        let b = sim.add_player_with("B", 3, 1, 0, 0, tank).unwrap();
        let c = sim.center();
        let pick_all = |sim: &mut Sim| {
            while sim.phase() == SimPhase::LevelUp {
                let ids: Vec<Id> = sim.player_ids().collect();
                for id in ids {
                    if !sim.offer_of(id).is_empty() {
                        sim.choose(id, 0);
                    }
                }
            }
        };
        for _ in 0..(90 * 30) {
            hero_at(&mut sim, a, c);
            hero_at(&mut sim, b, c + Vec2::new(2.0, 0.0));
            sim.step(1.0 / 30.0);
            pick_all(&mut sim);
        }
        let level = sim.run_of(a).unwrap().level;
        assert!(level > 2, "the party should have levelled in 90 s");

        // A third hero drops in: the party's level, and a card for each level missed.
        let late = sim.add_player("Late", 0, 1, 0, 0).unwrap();
        assert_eq!(sim.phase(), SimPhase::LevelUp);
        let mut picks = 0;
        while !sim.offer_of(late).is_empty() {
            sim.choose(late, 0);
            picks += 1;
        }
        assert_eq!(picks, level - 1);
        assert_eq!(sim.run_of(late).unwrap().level, level);
        assert_eq!(sim.phase(), SimPhase::Playing);

        // The first hero leaves mid-fight: what the others own still points at them.
        sim.remove_player(a);
        for _ in 0..(10 * 30) {
            hero_at(&mut sim, b, c);
            hero_at(&mut sim, late, c + Vec2::new(1.0, 1.0));
            sim.step(1.0 / 30.0);
            pick_all(&mut sim);
        }
        assert_eq!(sim.player_count(), 2);
        assert!(sim.areas.iter().all(|x| x.owner < 2));
        assert!(sim.totems.iter().all(|x| x.owner < 2));
        assert!(sim.pending.iter().all(|(_, h)| h.source < 2));
    }

    #[test]
    fn a_lone_hero_falls_and_the_run_ends() {
        let mut sim = Sim::new(0, 9);
        let id = sim.add_player("Solo", 0, 1, 0, 0).unwrap();
        sim.players[0].run.hp = 0.0;
        let c = sim.center();
        for _ in 0..(4 * 30) {
            hero_at(&mut sim, id, c);
            sim.step(1.0 / 30.0);
        }
        assert_eq!(sim.phase(), SimPhase::Over);
        assert!(!sim.snapshot().won);
    }
}
