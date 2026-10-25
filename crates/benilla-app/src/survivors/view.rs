//! The run on screen: every enemy, gem, totem, ground area and teammate in the session's snapshot
//! as a local unit the attach, animation and spell-visual systems dress like a streamed one, and
//! the session's events as the spell kits, swings and numbers a server's would play.
//!
//! A co-op field is drawn between the two newest snapshots, so it moves smoothly at the frame
//! rate whatever the server's send rate; a solo field is the sim's own, this frame.

use std::collections::{HashMap, HashSet};

use benilla_world::rig_anim::GlobalSeqDrive;
use benilla_world::world_point::WorldPoint;
use bevy::prelude::*;
use survivors_sim::{data, EnemySnap, Event, GemLook, Id, PlayerSnap, Snapshot, Vec2};

use super::hero::{self, Hero};
use super::session::Session;
use super::units::{self, Fx, Marks, UnitSpec};
use super::{Run, RunEntity};
use crate::creature_anim::{move_flags, MovementState};
use crate::net::{FieldChanged, Guid, ObjectStore};

/// A unit, object or area the sim knows by this id.
#[derive(Component, Clone, Copy)]
pub(super) struct SimId(pub Id);

/// Which kind of sim thing an entity draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Enemy,
    Gem,
    Totem,
    Area,
    Teammate,
    Me,
}

/// The sim's ids to their entities, for the events.
#[derive(Resource, Default)]
pub(super) struct Ids(HashMap<Id, (Entity, Kind)>);

impl Ids {
    pub fn clear(&mut self) {
        self.0.clear();
    }
    fn entity(&self, id: Id) -> Option<Entity> {
        self.0.get(&id).map(|e| e.0)
    }
}

/// An enemy on screen. The minimap's dots and the auto-runner read it.
#[derive(Component)]
pub(super) struct Enemy {
    pub dead: bool,
    pub radius: f32,
}

/// A pickup on screen.
#[derive(Component)]
pub(super) struct Gem;

/// Another player's hero.
#[derive(Component)]
pub(super) struct Teammate {
    pub id: Id,
    pub down: bool,
}

/// Where a thing stands between the previous snapshot and the newest, `t` of the way.
fn between(prev: Option<Vec2>, now: Vec2, t: f32) -> Vec2 {
    match prev {
        // A jump (a stray brought back, a blink) is not a slide.
        Some(p) if (p - now).length() < 8.0 => p.lerp(now, t),
        _ => now,
    }
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    a + d * t
}

fn gem_look(look: GemLook) -> (u32, f32) {
    match look {
        // `TurkeyLeg.mdx`.
        GemLook::Heal => (114, 2.5),
        // `BFD_WispSmallPurple.mdx`.
        GemLook::Purple => (1307, 1.0),
        // `BFD_WispSmallGreen.mdx`.
        GemLook::Green => (1267, 1.0),
        // `BFD_WispSmall.mdx`.
        GemLook::Small => (426, 1.0),
        // `GoldPilesmall01.mdx`.
        GemLook::Gold => (2614, 0.6),
    }
}

fn enemy_def(map: usize, e: &EnemySnap) -> Option<data::EnemyDef> {
    let roster = data::roster(map);
    if e.boss() {
        roster.bosses.get(e.roster_index()).copied()
    } else {
        roster.enemies.get(e.roster_index()).copied()
    }
}

/// Bring the entities in line with the snapshot.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn sync(
    mut commands: Commands,
    real: Res<Time<Real>>,
    session: Res<Session>,
    mut run: ResMut<Run>,
    mut ids: ResMut<Ids>,
    world: WorldPoint,
    hero: Query<Entity, With<Hero>>,
    mut units: Query<
        (
            &mut Transform,
            Option<&mut MovementState>,
            Option<&mut ObjectStore>,
            Option<&mut Marks>,
            Option<&mut Enemy>,
            Option<&mut Teammate>,
            &Guid,
        ),
        Without<Hero>,
    >,
    mut me: Query<(&Guid, &mut ObjectStore, &mut Marks), With<Hero>>,
    mut fields: MessageWriter<FieldChanged>,
    mut items: ResMut<crate::items::Items>,
    char_create: Option<Res<crate::entities::CharCreate>>,
) {
    let Some((at, snap)) = &session.snap else {
        return;
    };
    let Some(me_id) = session.me else { return };
    // The local hero is the menu's preview hero, already standing: it only needs its id.
    if let Ok(e) = hero.single() {
        ids.0.insert(me_id, (e, Kind::Me));
    }
    let t = match &session.prev {
        Some((before, _)) if *at > *before => {
            let span = (*at - *before).max(1e-3);
            ((real.elapsed_secs_f64() - *at) / span).clamp(0.0, 1.0) as f32
        }
        _ => 1.0,
    };
    let prev = session.prev.as_ref().map(|(_, s)| s);
    let origin = snap.origin;
    let ground = |p: Vec2, fallback: f32| {
        let v = Vec3::new(p.x, fallback, p.y);
        world.terrain_height_under(v).unwrap_or(fallback)
    };
    let base_y = snap
        .players
        .iter()
        .find(|p| p.id == me_id)
        .map_or(0.0, |p| p.pos.y);

    let mut seen: HashSet<Id> = HashSet::with_capacity(snap.enemies.len() + snap.gems.len() + 8);

    // Enemies.
    let prev_enemies: HashMap<Id, &EnemySnap> = prev
        .map(|p| p.enemies.iter().map(|e| (e.id, e)).collect())
        .unwrap_or_default();
    for e in &snap.enemies {
        seen.insert(e.id);
        let now = e.point(origin);
        let pos = between(prev_enemies.get(&e.id).map(|p| p.point(origin)), now, t);
        let yaw = prev_enemies
            .get(&e.id)
            .map_or(e.yaw(), |p| lerp_angle(p.yaw(), e.yaw(), t));
        let entity = match ids.0.get(&e.id) {
            Some((entity, _)) => *entity,
            None => {
                if e.dead {
                    continue;
                }
                let Some(def) = enemy_def(usize::from(snap.map), e) else {
                    continue;
                };
                let serial = run.guid();
                let y = ground(pos, base_y);
                let entity = units::spawn_unit(
                    &mut commands,
                    &UnitSpec {
                        serial,
                        display: def.display,
                        scale: def.scale,
                        level: def.level,
                        faction: units::FACTION_MONSTER,
                        bytes0: 1 << 8,
                        weapons: &[],
                        sheath: 0,
                    },
                    Vec3::new(pos.x, y, pos.y),
                    yaw,
                );
                commands.entity(entity).insert((
                    SimId(e.id),
                    Enemy {
                        dead: false,
                        radius: (if def.big { 1.3 } else { 0.6 }) * def.scale,
                    },
                    Marks::debuffs(),
                ));
                ids.0.insert(e.id, (entity, Kind::Enemy));
                continue;
            }
        };
        let Ok((mut tf, mv, store, marks, enemy, _, guid)) = units.get_mut(entity) else {
            continue;
        };
        // A corpse lies where it fell, and a unit standing still is not touched: a changed
        // transform costs the propagation and every reader downstream.
        let still = enemy.as_ref().is_some_and(|en| en.dead);
        if !still {
            let flat_now = Vec2::new(tf.translation.x, tf.translation.z);
            if flat_now.distance_squared(pos) > 1e-6 {
                let y = ground(pos, tf.translation.y);
                tf.translation = Vec3::new(pos.x, y, pos.y);
            }
            let rot = Quat::from_rotation_y(yaw);
            if !tf.rotation.abs_diff_eq(rot, 1e-4) {
                tf.rotation = rot;
            }
        }
        if let Some(mut mv) = mv {
            let speed = e.run_speed();
            let flags = if speed > 0.0 { move_flags::FORWARD } else { 0 };
            if mv.speed != speed || mv.flags != flags {
                mv.speed = speed;
                mv.flags = flags;
            }
        }
        if let (Some(mut enemy), Some(mut store), Some(mut marks)) = (enemy, store, marks) {
            if e.dead && !enemy.dead {
                enemy.dead = true;
                units::kill_unit(&mut store);
                marks.clear();
            } else if !e.dead {
                marks.set(&e.marks);
            }
            marks.flush(entity, guid.0, &mut store, &mut fields);
        }
    }

    // Gems.
    let prev_gems: HashMap<Id, Vec2> = prev
        .map(|p| p.gems.iter().map(|g| (g.id, g.point(origin))).collect())
        .unwrap_or_default();
    for g in &snap.gems {
        seen.insert(g.id);
        let pos = between(prev_gems.get(&g.id).copied(), g.point(origin), t);
        let lift = if g.to.is_some() { 1.0 } else { 0.5 };
        match ids.0.get(&g.id) {
            Some((entity, _)) => {
                if let Ok((mut tf, ..)) = units.get_mut(*entity) {
                    if Vec2::new(tf.translation.x, tf.translation.z).distance_squared(pos) > 1e-6 {
                        let y = ground(pos, tf.translation.y - lift) + lift;
                        tf.translation = Vec3::new(pos.x, y, pos.y);
                    }
                }
            }
            None => {
                let (display, scale) = gem_look(g.look);
                let serial = run.guid();
                let y = ground(pos, base_y);
                let entity = units::spawn_object(
                    &mut commands,
                    serial,
                    display,
                    scale,
                    Vec3::new(pos.x, y + lift, pos.y),
                );
                commands.entity(entity).insert((SimId(g.id), Gem));
                ids.0.insert(g.id, (entity, Kind::Gem));
            }
        }
    }

    // Totems.
    for totem in &snap.totems {
        seen.insert(totem.id);
        if ids.0.contains_key(&totem.id) {
            continue;
        }
        let serial = run.guid();
        let y = ground(totem.pos, base_y);
        let entity = units::spawn_unit(
            &mut commands,
            &UnitSpec {
                serial,
                display: totem.display,
                scale: 1.0,
                level: 60,
                faction: units::FACTION_FRIENDLY,
                bytes0: 0,
                weapons: &[],
                sheath: 0,
            },
            Vec3::new(totem.pos.x, y, totem.pos.y),
            0.0,
        );
        commands.entity(entity).insert(SimId(totem.id));
        ids.0.insert(totem.id, (entity, Kind::Totem));
    }

    // Ground areas.
    for area in &snap.areas {
        seen.insert(area.id);
        if ids.0.contains_key(&area.id) {
            continue;
        }
        let y = ground(area.pos, base_y);
        let at = Vec3::new(area.pos.x, y, area.pos.y);
        let entity = if area.overhead {
            commands
                .spawn((Transform::from_translation(at), RunEntity))
                .id()
        } else {
            let serial = run.guid();
            units::spawn_area_object(&mut commands, serial, area.spell, area.radius, at)
        };
        commands.entity(entity).insert(SimId(area.id));
        ids.0.insert(area.id, (entity, Kind::Area));
    }

    // The party.
    let prev_players: HashMap<Id, &PlayerSnap> = prev
        .map(|p| p.players.iter().map(|q| (q.id, q)).collect())
        .unwrap_or_default();
    for p in &snap.players {
        seen.insert(p.id);
        if p.id == me_id {
            if let Ok((guid, mut store, mut marks)) = me.single_mut() {
                let entity = ids.entity(me_id);
                if p.down {
                    marks.clear();
                } else {
                    marks.set(&p.buffs);
                }
                apply_down(&mut store, p.down);
                if let Some(entity) = entity {
                    marks.flush(entity, guid.0, &mut store, &mut fields);
                }
            }
            continue;
        }
        let before = prev_players.get(&p.id);
        let flat = between(
            before.map(|b| survivors_sim::flat(b.pos)),
            survivors_sim::flat(p.pos),
            t,
        );
        // On the ground unless mid-jump: this client's own terrain decides, whatever was reported.
        let reported = before.map_or(p.pos.y, |b| b.pos.y + (p.pos.y - b.pos.y) * t);
        let floor = ground(flat, reported);
        let y = if p.flags & move_flags::FALLING != 0 {
            reported.max(floor)
        } else {
            floor
        };
        let pos = Vec3::new(flat.x, y, flat.y);
        let yaw = before.map_or(p.yaw, |b| lerp_angle(b.yaw, p.yaw, t));
        let entity = match ids.0.get(&p.id) {
            Some((entity, _)) => *entity,
            None => {
                let class = data::CLASSES
                    .iter()
                    .find(|c| c.id == p.class)
                    .unwrap_or(&data::CLASSES[0]);
                let display = if p.display != 0 {
                    p.display
                } else {
                    class.looks.first().copied().unwrap_or(0)
                };
                let serial = run.guid();
                let mut look = p.look;
                if look.race == 0 {
                    look.race = p.race;
                    look.sex = p.sex;
                }
                let entity = hero::spawn_teammate(
                    &mut commands,
                    class,
                    display,
                    hero::Dress {
                        look,
                        gear: p.gear,
                        items: &mut items,
                        char_create: char_create.as_deref(),
                    },
                    serial,
                    pos,
                );
                commands.entity(entity).insert((
                    SimId(p.id),
                    Teammate {
                        id: p.id,
                        down: false,
                    },
                ));
                ids.0.insert(p.id, (entity, Kind::Teammate));
                continue;
            }
        };
        let Ok((mut tf, mv, store, marks, _, mate, guid)) = units.get_mut(entity) else {
            continue;
        };
        if tf.translation != pos {
            tf.translation = pos;
        }
        // The body leans off the facing as the teammate strafes, as their own client draws it.
        let lean = if p.down {
            0.0
        } else {
            crate::creature_anim::strafe_body_offset(p.flags)
        };
        let rot = Quat::from_rotation_y(yaw + lean);
        if !tf.rotation.abs_diff_eq(rot, 1e-4) {
            tf.rotation = rot;
        }
        if let Some(mut mv) = mv {
            let (speed, flags) = if p.down { (0.0, 0) } else { (p.speed, p.flags) };
            if mv.speed != speed || mv.flags != flags || mv.vertical_speed != p.vy {
                mv.speed = speed;
                mv.flags = flags;
                mv.vertical_speed = p.vy;
            }
        }
        if let (Some(mut store), Some(mut marks)) = (store, marks) {
            if p.down {
                marks.clear();
            } else {
                marks.set(&p.buffs);
            }
            apply_down(&mut store, p.down);
            marks.flush(entity, guid.0, &mut store, &mut fields);
        }
        if let Some(mut mate) = mate {
            mate.down = p.down;
        }
    }

    // Whatever the snapshot no longer has: corpses (the sim keeps them while the death plays),
    // totems and those who left fade; the rest simply go.
    ids.0.retain(|id, (entity, kind)| {
        if seen.contains(id) || *kind == Kind::Me {
            return true;
        }
        match kind {
            Kind::Enemy | Kind::Totem | Kind::Teammate => {
                commands
                    .entity(*entity)
                    .try_insert(benilla_world::model_fade::DespawnFade::default());
            }
            Kind::Gem | Kind::Area => commands.entity(*entity).try_despawn(),
            Kind::Me => {}
        }
        false
    });
}

/// A fallen hero lies dead (zero health plays Death and holds it); a revived one stands.
fn apply_down(store: &mut ObjectStore, down: bool) {
    let dead = units::health(store) == 0;
    if down && !dead {
        units::kill_unit(store);
    } else if !down && dead {
        units::revive_unit(store);
    }
}

/// The session's events as a server's would play: cast kits, missiles, swings, numbers, the
/// teleports of the local hero's Blink and Charge.
#[allow(clippy::type_complexity)]
pub(super) fn play_events(
    mut session: ResMut<Session>,
    ids: Res<Ids>,
    world: WorldPoint,
    mut hero: Query<(&mut Transform, &mut Hero)>,
    mut rig: ResMut<hero::CamRig>,
    mut fx: Fx,
    mut loot: MessageWriter<super::progress::BossLoot>,
) {
    if session.events.is_empty() {
        return;
    }
    let me = session.me;
    let events = std::mem::take(&mut session.events);
    let ground = |p: Vec2| {
        let v = Vec3::new(p.x, 0.0, p.y);
        Vec3::new(p.x, world.terrain_height_under(v).unwrap_or(0.0), p.y)
    };
    for event in events {
        match event {
            Event::Cast {
                caster,
                spell,
                targets,
                dest,
            } => {
                let Some(caster) = ids.entity(caster) else {
                    continue;
                };
                let hits: Vec<Entity> = targets.iter().filter_map(|t| ids.entity(*t)).collect();
                fx.cast(caster, spell, &hits, dest.map(ground));
            }
            Event::Impact { target, spell } => {
                if let Some(target) = ids.entity(target) {
                    fx.impact(target, spell);
                }
            }
            Event::Swing {
                attacker,
                victim,
                damage,
            } => {
                if let (Some(a), Some(v)) = (ids.entity(attacker), ids.entity(victim)) {
                    fx.swing(a, v, damage as f32);
                }
            }
            Event::Number {
                anchor,
                amount,
                crit,
                color,
            } => {
                if let Some(anchor) = ids.entity(anchor) {
                    fx.number(anchor, amount as f32, crit, color);
                }
            }
            Event::Teleport {
                player,
                to,
                yaw,
                seq,
            } => {
                if Some(player) != me {
                    continue;
                }
                if let Ok((mut tf, mut hm)) = hero.single_mut() {
                    let at = ground(to);
                    tf.translation = at;
                    tf.rotation = Quat::from_rotation_y(yaw);
                    // A Charge turns the hero onto its target: the camera turns with him.
                    rig.yaw = crate::creature_anim::wrap_pi(rig.yaw + (yaw - hm.face));
                    hm.face = yaw;
                    hm.model = yaw;
                }
                session.seq = session.seq.max(seq);
            }
            Event::Revived { player, by } => {
                // The priest's Resurrection kit, from the one who stood over them.
                if let (Some(target), Some(caster)) = (ids.entity(player), ids.entity(by)) {
                    fx.cast(caster, 2006, &[target], None);
                }
            }
            Event::BossDown { map, .. } => {
                loot.write(super::progress::BossLoot { map });
            }
            Event::Banner { .. } | Event::Downed { .. } => {}
        }
    }
}

/// Pickups hang still: a wisp's own loop (its circling motes and their trails) stops on its first
/// frame, clip and global sequences alike, as soon as the model attaches.
pub(super) fn still_gems(
    mut players: Query<&mut AnimationPlayer, (With<Gem>, Added<AnimationPlayer>)>,
    mut drives: Query<&mut GlobalSeqDrive, (With<Gem>, Added<GlobalSeqDrive>)>,
) {
    for mut player in &mut players {
        for (_, active) in player.playing_animations_mut() {
            active.set_speed(0.0);
        }
    }
    for mut drive in &mut drives {
        drive.set_paused(true);
    }
}

/// The latest snapshot's player rows, for the party frame.
pub(super) fn party(snap: &Snapshot) -> &[PlayerSnap] {
    &snap.players
}
