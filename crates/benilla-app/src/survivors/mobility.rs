//! The class's mobility spell: learned at [`survivors_sim::LEARN_LEVEL`], cast only on `F`, never
//! by itself. Each is the class's own signature escape: a mage blinks, a rogue sprints, a warrior
//! charges. The session casts it (the sim, solo, or the server in co-op), and a Blink or Charge
//! comes back as a teleport the view applies to the hero.

use bevy::prelude::*;

use super::coop::Net;
use super::hero;
use super::session::Session;
use super::{Phase, Run};

pub(super) const KEY: KeyCode = KeyCode::KeyF;

pub(super) fn cast_mobility(
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    run: Res<Run>,
    aim: Res<hero::AimPoint>,
    mut session: ResMut<Session>,
    mut net: ResMut<Net>,
) {
    if *phase.get() != Phase::Playing || run.dead_t.is_some() || !keys.just_pressed(KEY) {
        return;
    }
    if run.mobility().is_none() || run.mobile.cd > 0.0 {
        return;
    }
    let aim = aim.0.map(survivors_sim::flat);
    session.cast_mobility(aim, &mut net);
}
