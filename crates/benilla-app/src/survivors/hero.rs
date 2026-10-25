//! The hero: its body, World of Warcraft's movement over the terrain (1.12's control model:
//! W/S run and backpedal, A/D turn, Q/E strafe, the right mouse steers, the left mouse looks
//! around, both run), its third-person camera, and its health.

use benilla_world::view::WorldCamera;
use benilla_world::world_point::WorldPoint;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use super::data::{self, ClassDef};
use super::units::{self, flat, wow_to_bevy, yaw_toward, Marks, UnitSpec};
use super::{Phase, Run, Selection};
use crate::creature_anim::{
    ease_strafe_yaw, move_flags, strafe_body_offset, wrap_pi, MovementState,
};

#[derive(Component, Default)]
pub(super) struct Hero {
    /// Vertical velocity in yards/second while airborne; 0 on the ground.
    pub(super) vy: f32,
    /// Height above the ground while airborne. Kept here, not in the transform: the creature
    /// ground clamp (`net::motion::ground_clamp_creatures`) re-seats every unit on the floor at
    /// the top of the frame, so the arc is re-applied over the ground each frame.
    pub(super) air: f32,
    /// The facing (yaw, left positive): what W runs along, A/D turn and the right mouse steers.
    /// The sim reads this one; the transform carries the rendered body, which leans off it.
    pub(super) face: f32,
    /// The rendered body's yaw: the facing, ±45°/±90° while strafing, lagging it standing still.
    pub(super) model: f32,
    /// Horizontal velocity at takeoff: a jump's direction is committed when it leaves the ground.
    pub(super) carry: Vec3,
}

/// The body radius enemies stop at.
pub(super) const HERO_RADIUS: f32 = 0.7;

/// The hop's initial upward speed and the gravity that brings it back down — the reference
/// `CMovement` jump seed and gravity (`player/state.rs`'s `JUMP_SPEED` / `GRAVITY`).
const JUMP_SPEED: f32 = 7.955_547;
const GRAVITY: f32 = 19.291_105;

/// The 1.12 control model's rates, as `player/state.rs` has them: backpedal over run (4.5 / 7.0),
/// the keyboard turn (π rad/s, 0.75× while moving or airborne) and the standing body's chase.
const BACK_RATIO: f32 = 4.5 / 7.0;
const TURN_RATE: f32 = std::f32::consts::PI;
const TURN_RATE_MOVING: f32 = 0.75;
const STATIONARY_CHASE_RATE: f32 = 8.0;

/// Radians of camera turn per raw mouse unit at Mouse Look Speed 180 and Mouse Sensitivity 1
/// (`player/camera.rs`'s `LOOK_SENSITIVITY`).
const LOOK_SENSITIVITY: f32 = 0.003;
/// The camera's pitch clamp, ±89° (`CAM_PITCH_LIMIT`); a low camera is pulled in off the ground.
const PITCH_LIMIT: f32 = 89.0 * std::f32::consts::PI / 180.0;
/// Where a run's camera starts: behind the hero, a little above.
const START_PITCH: f32 = 20.0 * std::f32::consts::PI / 180.0;
const START_DIST: f32 = 20.0;
/// Wheel zoom: the reference's 1 yd a notch, glided at `cameraDistanceMoveSpeed`, under its 50 yd
/// ceiling. [`CamRig::zoom_max`] is the Max Camera Distance slider.
const ZOOM_MIN: f32 = 1.0;
pub(super) const ZOOM_MAX: f32 = 50.0;
const ZOOM_STEP: f32 = 1.0;
const ZOOM_SPEED: f32 = 8.33;
/// The framing pivot over the feet: about a human's neck.
const PIVOT: f32 = 1.8;

/// Camera Following Style (1.12's `cameraSmoothStyle`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(super) enum Follow {
    /// Swings back behind the hero only while he is driven: the reference's default.
    #[default]
    Smart,
    /// Swings back whenever it is let go.
    Always,
    /// Stays where it was put.
    Never,
}

#[derive(Resource)]
pub(super) struct CamRig {
    /// The distance the camera sits at, gliding to [`Self::want`].
    pub dist: f32,
    pub want: f32,
    /// The menu's slow orbit.
    pub orbit: f32,
    /// The camera's own yaw (absolute, the hero's convention): the left mouse swings it free of
    /// the facing, the right mouse turns the hero to it, and a keyboard turn carries it.
    pub yaw: f32,
    /// Elevation above the horizon (radians), ±89°.
    pub pitch: f32,
    /// The mouse buttons held on the world, not on the UI: they own the cursor while down.
    pub left: bool,
    pub right: bool,
    /// Where the cursor was when the look began, put back on release.
    stash: Option<Vec2>,
    /// TOGGLEAUTORUN's latch (Num Lock, mouse button 4).
    pub autorun: bool,
    // The options pane's knobs.
    pub look_speed: f32,
    pub sensitivity: f32,
    pub invert_pitch: bool,
    pub follow: Follow,
    /// AUTO_FOLLOW_SPEED, degrees per second of the swing back.
    pub follow_speed: f32,
    pub zoom_max: f32,
}

impl Default for CamRig {
    fn default() -> Self {
        Self {
            dist: START_DIST,
            want: START_DIST,
            orbit: 0.6,
            yaw: 0.0,
            pitch: START_PITCH,
            left: false,
            right: false,
            stash: None,
            autorun: false,
            look_speed: 180.0,
            sensitivity: 1.0,
            invert_pitch: false,
            follow: Follow::Smart,
            follow_speed: 180.0,
            zoom_max: ZOOM_MAX,
        }
    }
}

impl CamRig {
    /// Either button turning the view.
    pub fn looking(&self) -> bool {
        self.left || self.right
    }

    fn yaw_rate(&self) -> f32 {
        LOOK_SENSITIVITY * self.look_speed / 180.0 * self.sensitivity
    }
}

/// Back at the menu: the next run's camera starts behind the hero, at the zoom the player left.
pub(super) fn reset_camera(mut rig: ResMut<CamRig>) {
    rig.yaw = 0.0;
    rig.pitch = START_PITCH;
    rig.autorun = false;
}

/// Where a battleground's runs start.
pub(super) fn arena(map: &data::MapDef) -> Vec3 {
    wow_to_bevy(map.center)
}

/// The world is the battleground being stood in ([`Selection::stage`]): entering one on another
/// continent worldports, which raises the loading screen until the new ground is in.
pub(super) fn follow_map(
    selection: Res<Selection>,
    current: Option<ResMut<benilla_world::world_map::CurrentMap>>,
) {
    let Some(mut current) = current else {
        return;
    };
    let want = selection.stage_def().map;
    if current.0 != want {
        current.0 = want;
    }
}

/// The hero in the class trainer's look, weapons drawn.
pub(super) fn spawn_hero(
    commands: &mut Commands,
    class: &'static ClassDef,
    display: u32,
    race: u8,
    sex: u8,
    serial: u64,
    pos: Vec3,
) -> Entity {
    let bytes0 = u32::from(race)
        | u32::from(class.id) << 8
        | u32::from(sex) << 16
        | u32::from(class.power) << 24;
    let e = units::spawn_unit(
        commands,
        &UnitSpec {
            serial,
            display,
            scale: 1.0,
            level: 60,
            faction: units::FACTION_FRIENDLY,
            bytes0,
            weapons: class.weapons,
            sheath: class.sheath,
        },
        pos,
        0.0,
    );
    commands.entity(e).insert((Hero::default(), Marks::buffs()));
    e
}

/// Another player's hero: the same body, driven by the snapshots instead of the keyboard.
pub(super) fn spawn_teammate(
    commands: &mut Commands,
    class: &'static ClassDef,
    display: u32,
    race: u8,
    sex: u8,
    serial: u64,
    pos: Vec3,
) -> Entity {
    let e = spawn_hero(commands, class, display, race, sex, serial, pos);
    commands.entity(e).remove::<Hero>();
    e
}

/// Midday over the battleground, whatever the clock says.
pub(super) const MINUTE_OF_DAY: u32 = 780;

pub(super) fn daylight(debug: Option<ResMut<benilla_world::dev_state::DebugState>>) {
    let Some(mut debug) = debug else { return };
    if debug.lighting.follow_server_time || debug.lighting.manual_minute != MINUTE_OF_DAY {
        debug.lighting.follow_server_time = false;
        debug.lighting.manual_minute = MINUTE_OF_DAY;
    }
}

/// The mouse on the world: a button pressed off the UI takes the cursor (locked and hidden until
/// it is let go), the left one swings the camera around the hero, the right one swings it and
/// turns the hero with it.
#[allow(clippy::too_many_arguments)]
pub(super) fn mouse_look(
    phase: Res<State<Phase>>,
    run: Res<Run>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    ui: Query<&Interaction>,
    mut window: Query<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut rig: ResMut<CamRig>,
    mut hero: Query<&mut Hero>,
) {
    let Ok((mut window, mut cursor)) = window.single_mut() else {
        return;
    };
    let playing = *phase.get() == Phase::Playing;
    // A press on a button or a card is the UI's, never the camera's.
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let was = rig.looking();
    let held = |b: MouseButton, before: bool| {
        if !playing || !mouse.pressed(b) {
            false
        } else if mouse.just_pressed(b) {
            !over_ui
        } else {
            before
        }
    };
    rig.left = held(MouseButton::Left, rig.left);
    rig.right = held(MouseButton::Right, rig.right);
    // Both buttons down runs forward: like a W key-down, it ends autorun.
    if rig.left && rig.right && mouse.any_just_pressed([MouseButton::Left, MouseButton::Right]) {
        rig.autorun = false;
    }
    let looking = rig.looking();
    if looking && !was {
        rig.stash = window.cursor_position();
    }
    let grab = looking && window.focused;
    if grab && cursor.grab_mode != CursorGrabMode::Locked {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    } else if !grab && cursor.grab_mode == CursorGrabMode::Locked {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
        // The browser puts a pointer-locked cursor back itself, and cannot warp one.
        #[cfg(not(target_arch = "wasm32"))]
        if !looking {
            if let Some(at) = rig.stash.take() {
                window.set_cursor_position(Some(at));
            }
        }
    }
    if !grab {
        return;
    }
    let rate = rig.yaw_rate();
    let dy = if rig.invert_pitch {
        -motion.delta.y
    } else {
        motion.delta.y
    };
    rig.yaw = wrap_pi(rig.yaw - motion.delta.x * rate);
    rig.pitch = (rig.pitch + dy * rate).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    if rig.right && run.dead_t.is_none() {
        if let Ok(mut hm) = hero.single_mut() {
            hm.face = rig.yaw;
        }
    }
}

/// World of Warcraft's movement over the terrain within the battleground's leash, and the
/// streaming focus on the hero. W runs along the facing and S backpedals at 4.5/7 of it; A/D turn
/// (strafe while the right mouse is held), Q/E strafe, both mouse buttons run, Num Lock runs on its
/// own. A jump keeps the velocity it left the ground with.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_hero(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    phase: Res<State<Phase>>,
    run: Res<Run>,
    selection: Res<Selection>,
    mut rig: ResMut<CamRig>,
    world: WorldPoint,
    auto: Option<Res<super::auto::Auto>>,
    mut player: ResMut<crate::player::Player>,
    mut hero: Query<(&mut Transform, &mut MovementState, &mut Hero)>,
) {
    let focus = match hero.single_mut() {
        Ok((mut tf, mut mv, mut hm)) => {
            let dt = time.delta_secs();
            let playing = *phase.get() == Phase::Playing && run.dead_t.is_none();
            let airborne = hm.vy != 0.0 || hm.air > 0.0;
            let run_speed = run.stats().speed * run.haste_mult();
            let mut flags = 0;
            let mut vel = Vec3::ZERO;
            let mut steering = false;
            if let Some(auto) = auto.as_deref().filter(|_| playing) {
                // Driven: it faces where it runs.
                let dir = super::auto::steer(auto);
                if dir != Vec3::ZERO {
                    let dir = dir.normalize();
                    hm.face = yaw_toward(dir);
                    vel = dir * run_speed;
                    flags = move_flags::FORWARD;
                }
            } else if playing {
                if keys.just_pressed(KeyCode::NumLock) || mouse.just_pressed(MouseButton::Forward) {
                    rig.autorun = !rig.autorun;
                }
                // A W or S key-down ends autorun; letting S go after it leaves the hero standing.
                if keys.any_just_pressed([
                    KeyCode::KeyW,
                    KeyCode::ArrowUp,
                    KeyCode::KeyS,
                    KeyCode::ArrowDown,
                ]) {
                    rig.autorun = false;
                }
                let forward = keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp])
                    || rig.autorun
                    || (rig.left && rig.right);
                let back = keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]);
                let fwd = i32::from(forward) - i32::from(back);
                // The right mouse makes the turn keys strafe, as the facing follows the camera.
                steering = rig.right;
                let turn_left = keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]);
                let turn_right = keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]);
                let mut side =
                    i32::from(keys.pressed(KeyCode::KeyE)) - i32::from(keys.pressed(KeyCode::KeyQ));
                let mut turn = 0;
                if steering {
                    side += i32::from(turn_right) - i32::from(turn_left);
                } else {
                    turn = i32::from(turn_left) - i32::from(turn_right);
                }
                let side = side.signum();
                let translating = fwd != 0 || side != 0;
                if turn != 0 {
                    let slowed = translating || airborne;
                    let rate = TURN_RATE * if slowed { TURN_RATE_MOVING } else { 1.0 };
                    let delta = turn as f32 * rate * dt;
                    hm.face = wrap_pi(hm.face + delta);
                    // The camera's yaw is held relative to the facing: a keyboard turn carries it.
                    rig.yaw = wrap_pi(rig.yaw + delta);
                    flags |= if turn > 0 {
                        move_flags::TURN_LEFT
                    } else {
                        move_flags::TURN_RIGHT
                    };
                }
                if translating {
                    let rot = Quat::from_rotation_y(hm.face);
                    let dir = (rot * Vec3::NEG_Z) * fwd as f32 + (rot * Vec3::X) * side as f32;
                    // Any backward component runs at the backpedal speed, diagonals included.
                    let speed = if fwd < 0 {
                        run_speed * BACK_RATIO
                    } else {
                        run_speed
                    };
                    vel = dir.normalize() * speed;
                    flags |= match fwd {
                        1 => move_flags::FORWARD,
                        -1 => move_flags::BACKWARD,
                        _ => 0,
                    } | match side {
                        1 => move_flags::STRAFE_RIGHT,
                        -1 => move_flags::STRAFE_LEFT,
                        _ => 0,
                    };
                }
                // Camera Following Style: the camera swings back behind a driven hero at the
                // follow speed, never while a mouse button holds it.
                let swing = match rig.follow {
                    Follow::Smart => flags != 0,
                    Follow::Always => true,
                    Follow::Never => false,
                };
                if swing && !rig.looking() && dt > 0.0 {
                    let gap = wrap_pi(hm.face - rig.yaw);
                    // A return lasts |gap| / speed, never under 0.1 s (`cameraSmoothTimeMin`).
                    let rate = rig.follow_speed.to_radians().max(gap.abs() / 0.1);
                    let step = (rate * dt).min(gap.abs());
                    rig.yaw = wrap_pi(rig.yaw + step.copysign(gap));
                }
            }
            let mut pos = tf.translation;
            // In the air the takeoff's velocity holds: the keys can turn the hero, not the arc.
            let step_vel = if airborne { hm.carry } else { vel };
            if dt > 0.0 {
                pos += step_vel * dt;
            }
            let moving = vel != Vec3::ZERO;
            mv.speed = vel.length();
            mv.flags = flags;
            let map = selection.stage_def();
            let center = arena(map);
            let off = flat(pos - center);
            if off.length() > map.leash * 2.0 {
                // Just entered another battleground: start at its centre.
                pos = center;
            } else if off.length() > map.leash {
                let held = center + off.normalize() * map.leash;
                pos.x = held.x;
                pos.z = held.z;
            }
            let ground = world.terrain_height_under(pos).unwrap_or(pos.y);
            // Space hops: lift off the terrain, then gravity brings the hero back down.
            if playing && keys.just_pressed(KeyCode::Space) && !airborne {
                hm.vy = JUMP_SPEED;
                hm.carry = vel;
            }
            if playing && (hm.vy != 0.0 || hm.air > 0.0) {
                hm.vy -= GRAVITY * dt;
                hm.air += hm.vy * dt;
                if hm.air <= 0.0 {
                    hm.air = 0.0;
                    hm.vy = 0.0;
                    hm.carry = Vec3::ZERO;
                }
            } else {
                hm.air = 0.0;
                hm.vy = 0.0;
                hm.carry = Vec3::ZERO;
            }
            pos.y = ground + hm.air;
            // Feed the animation driver: `vertical_speed`'s sign and the `FALLING` flag are what
            // select the Jump (37/38) / Fall (40) animation (`creature_anim::select`).
            mv.vertical_speed = hm.vy;
            let in_air = hm.vy != 0.0;
            if in_air {
                mv.flags |= move_flags::FALLING;
            }
            mv.flags = body_heading(&mut hm, mv.flags, dt, moving, in_air, steering);
            let rot = Quat::from_rotation_y(hm.model);
            if !tf.rotation.abs_diff_eq(rot, 1e-5) {
                tf.rotation = rot;
            }
            if tf.translation != pos {
                tf.translation = pos;
            }
            pos
        }
        Err(_) => {
            let mut p = arena(selection.stage_def());
            if let Some(h) = world.terrain_height_under(p) {
                p.y = h;
            }
            p
        }
    };
    // The terrain streams and the loading screen waits on the ground under this.
    player.active = true;
    player.detached = false;
    player.settling = false;
    player.world_stale = false;
    player.pos = focus;
}

/// The rendered body off the facing, as `player/gait.rs` drives the player's own: strafing leans
/// it ±90° (±45° on a diagonal, mirrored backpedaling), running or airborne snaps it to the
/// facing, and standing it lags a mouse turn up to 90°, then sweeps on at 8× the turn rate with
/// its feet shuffling. Returns the animation's flags.
fn body_heading(
    hm: &mut Hero,
    flags: u32,
    dt: f32,
    moving: bool,
    airborne: bool,
    steering: bool,
) -> u32 {
    let offset = strafe_body_offset(flags);
    let mut turned = 0.0_f32;
    if offset != 0.0 {
        hm.model = ease_strafe_yaw(hm.model, hm.face, offset, dt);
    } else if moving || airborne {
        hm.model = hm.face;
    } else {
        let delta = wrap_pi(hm.face - hm.model);
        let mut step = (delta.abs() - std::f32::consts::FRAC_PI_2).max(0.0);
        if !steering {
            step += dt * TURN_RATE * STATIONARY_CHASE_RATE;
        }
        turned = step.min(delta.abs()).copysign(delta);
        hm.model = wrap_pi(hm.model + turned);
    }
    let mut anim = flags;
    if !moving && !airborne {
        anim &= !(move_flags::TURN_LEFT | move_flags::TURN_RIGHT);
        if turned > 1e-5 {
            anim |= move_flags::TURN_LEFT;
        } else if turned < -1e-5 {
            anim |= move_flags::TURN_RIGHT;
        }
    }
    anim
}

/// A slow orbit at the menu; in play, WoW's third-person camera: behind the hero at the camera's
/// own yaw and pitch, the wheel gliding it in and out, pulled in where the ground would bury it.
#[allow(clippy::too_many_arguments)]
pub(super) fn place_camera(
    time: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    mut rig: ResMut<CamRig>,
    mut wheel: MessageReader<MouseWheel>,
    player: Res<crate::player::Player>,
    world: WorldPoint,
    hero: Query<&Transform, (With<Hero>, Without<WorldCamera>)>,
    mut cams: Query<&mut Transform, With<WorldCamera>>,
) {
    // With no hero yet, the focus `drive_hero` grounded.
    let focus = hero.single().map_or(player.pos, |t| t.translation);
    let dt = time.delta_secs();
    for w in wheel.read() {
        let notches = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y * 0.02,
        };
        rig.want -= notches * ZOOM_STEP;
    }
    // Lowering the Max Camera Distance pulls a farther camera in.
    rig.want = rig.want.clamp(ZOOM_MIN, rig.zoom_max);
    // The distance glides to the wheel's at a constant speed, not an ease.
    let gap = rig.want - rig.dist;
    rig.dist += gap.clamp(-ZOOM_SPEED * dt, ZOOM_SPEED * dt);
    let (eye, look) = if *phase.get() == Phase::Menu {
        rig.orbit += dt * 0.2;
        let d = 7.0;
        (
            focus + Vec3::new(rig.orbit.sin() * d, 2.4, rig.orbit.cos() * d),
            focus + Vec3::Y * 1.1,
        )
    } else {
        let (sy, cy) = rig.yaw.sin_cos();
        let (sp, cp) = rig.pitch.sin_cos();
        let look = focus + Vec3::Y * PIVOT;
        let back = Vec3::new(cp * sy, sp, cp * cy);
        // Pulled in toward the head where the ground would bury the lens, as the client's
        // collision corridor does, rather than lifted over it.
        const STEPS: usize = 24;
        let stride = rig.dist / STEPS as f32;
        let mut dist = rig.dist;
        for i in 1..=STEPS {
            let p = look + back * (stride * i as f32);
            if world.terrain_height_under(p).is_some_and(|h| p.y < h + 0.4) {
                dist = (stride * (i - 1) as f32).max(0.2);
                break;
            }
        }
        (look + back * dist, look)
    };
    let placed = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    for mut cam in &mut cams {
        *cam = placed;
    }
}
