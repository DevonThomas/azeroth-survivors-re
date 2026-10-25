//! The run as the client holds it. A solo run steps its own [`Sim`] here every frame; a co-op run
//! is the dedicated server's, and this keeps what it last said ([`super::coop::pump`] fills it).
//! Either way the rest of the mode reads the same three things: the latest [`Snapshot`], the
//! [`Event`]s since the last frame, and the local hero's [`super::Run`] and cards.

use bevy::prelude::*;
use survivors_sim::{Choice, Event, HeroInput, Id, Sim, SimPhase, Snapshot, Vec2};

use super::coop::Net;
use super::hero::Hero;
use super::hud::Offer;
use super::{cards, Phase, Run};
use crate::creature_anim::MovementState;

#[derive(Default)]
pub(super) enum Backend {
    #[default]
    Idle,
    Solo(Box<Sim>),
    /// The server's run: the snapshots and the hero arrive through [`super::coop::pump`].
    Online,
}

#[derive(Resource, Default)]
pub(super) struct Session {
    pub backend: Backend,
    /// The local hero's id in the run.
    pub me: Option<Id>,
    /// The newest snapshot and the one before it, each with the real time it arrived: the view
    /// draws a co-op field between the two.
    pub snap: Option<(f64, Snapshot)>,
    pub prev: Option<(f64, Snapshot)>,
    /// Events not yet shown.
    pub events: Vec<Event>,
    /// The cards on offer to the local hero.
    pub offer: Vec<Choice>,
    /// The last teleport the local hero took; reported back so the server drops older positions.
    pub seq: u32,
    /// A card was picked and the server has not dealt the next state yet…
    pub picked: bool,
    /// …which it has once the hero's pending levels drop below this.
    pub picked_from: u32,
    /// The run is over: the results screen is up or about to be.
    pub over: bool,
}

impl Session {
    pub fn online(&self) -> bool {
        matches!(self.backend, Backend::Online)
    }

    pub fn running(&self) -> bool {
        !matches!(self.backend, Backend::Idle)
    }

    pub fn latest(&self) -> Option<&Snapshot> {
        self.snap.as_ref().map(|(_, s)| s)
    }

    /// Start a solo run.
    pub fn start_solo(
        &mut self,
        map: usize,
        class: usize,
        race: u8,
        sex: u8,
        display: u32,
        loadout: survivors_sim::Loadout,
    ) {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        let mut sim = Sim::new(map, seed);
        self.me = sim.add_player_with("", class, race, sex, display, loadout);
        *self = Self {
            backend: Backend::Solo(Box::new(sim)),
            me: self.me,
            ..Self::default()
        };
    }

    /// The server put this client's hero `you` into a run.
    pub fn start_online(&mut self, you: Id) {
        *self = Self {
            backend: Backend::Online,
            me: Some(you),
            ..Self::default()
        };
    }

    pub fn end(&mut self) {
        *self = Self::default();
    }

    pub fn sim_mut(&mut self) -> Option<&mut Sim> {
        match &mut self.backend {
            Backend::Solo(sim) => Some(sim),
            _ => None,
        }
    }

    pub fn push_snapshot(&mut self, now: f64, snap: Snapshot) {
        // Unreliable delivery may reorder: an older tick than the newest is dropped.
        if let Some((_, s)) = &self.snap {
            if snap.tick <= s.tick && snap.phase != 2 {
                return;
            }
        }
        self.prev = self.snap.take();
        self.snap = Some((now, snap));
    }

    /// Take card `index` of the local hero's offer.
    pub fn choose(&mut self, index: usize, net: &mut Net, run: &mut Run) {
        let Some(me) = self.me else { return };
        match &mut self.backend {
            Backend::Solo(sim) => {
                sim.choose(me, index);
                self.offer = sim.offer_of(me).to_vec();
                if let Some(r) = sim.run_of(me) {
                    let next_guid = run.next_guid;
                    run.0 = survivors_sim::Run { next_guid, ..r };
                }
            }
            Backend::Online => {
                net.send(&survivors_net::ClientMsg::Choose { index: index as u8 });
                self.offer.clear();
                self.picked = true;
                self.picked_from = run.pending_levels;
            }
            Backend::Idle => {}
        }
    }

    /// The card tricks: a fresh hand, banishing card `index`, or passing the hand for gold.
    pub fn reroll(&mut self, net: &mut Net) {
        let Some(me) = self.me else { return };
        match &mut self.backend {
            Backend::Solo(sim) => {
                sim.reroll(me);
                self.offer = sim.offer_of(me).to_vec();
            }
            Backend::Online => net.send(&survivors_net::ClientMsg::Reroll),
            Backend::Idle => {}
        }
    }

    pub fn banish(&mut self, index: usize, net: &mut Net) {
        let Some(me) = self.me else { return };
        match &mut self.backend {
            Backend::Solo(sim) => {
                sim.banish(me, index);
                self.offer = sim.offer_of(me).to_vec();
            }
            Backend::Online => net.send(&survivors_net::ClientMsg::Banish { index: index as u8 }),
            Backend::Idle => {}
        }
    }

    pub fn skip(&mut self, net: &mut Net) {
        let Some(me) = self.me else { return };
        match &mut self.backend {
            Backend::Solo(sim) => {
                sim.skip(me);
                self.offer = sim.offer_of(me).to_vec();
            }
            Backend::Online => {
                net.send(&survivors_net::ClientMsg::Skip);
                self.offer.clear();
            }
            Backend::Idle => {}
        }
    }

    pub fn cast_mobility(&mut self, aim: Option<Vec2>, net: &mut Net) {
        let Some(me) = self.me else { return };
        match &mut self.backend {
            Backend::Solo(sim) => sim.cast_mobility(me, aim),
            Backend::Online => net.send(&survivors_net::ClientMsg::Mobility { aim }),
            Backend::Idle => {}
        }
    }
}

/// Report the local hero, step a solo run, and mirror the local hero's state into [`Run`].
#[allow(clippy::too_many_arguments)]
pub(super) fn advance(
    time: Res<Time>,
    real: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    mut session: ResMut<Session>,
    mut run: ResMut<Run>,
    mut net: ResMut<Net>,
    hero: Query<(&Transform, &MovementState, &Hero)>,
) {
    if *phase.get() == Phase::Menu {
        return;
    }
    let Some(me) = session.me else { return };
    let input = hero.single().ok().map(|(tf, mv, h)| {
        let fwd = tf.rotation * Vec3::NEG_Z;
        HeroInput {
            pos: tf.translation,
            yaw: survivors_sim::yaw_toward(Vec2::new(fwd.x, fwd.z)),
            flags: mv.flags,
            speed: mv.speed,
            vy: h.vy,
            seq: session.seq,
        }
    });
    let now = real.elapsed_secs_f64();
    let session = &mut *session;
    match &mut session.backend {
        Backend::Idle => {}
        Backend::Solo(sim) => {
            if let Some(input) = input {
                sim.set_input(me, input);
            }
            sim.step(time.delta_secs());
            session.events.extend(sim.drain_events());
            let snap = sim.snapshot();
            session.prev = None;
            session.snap = Some((now, snap));
            session.offer = sim.offer_of(me).to_vec();
            if let Some(r) = sim.run_of(me) {
                let next_guid = run.next_guid;
                run.0 = survivors_sim::Run { next_guid, ..r };
            }
            session.over = sim.phase() == SimPhase::Over;
        }
        Backend::Online => {
            if let Some(input) = input {
                net.send(&survivors_net::ClientMsg::Input(survivors_net::InputMsg {
                    pos: input.pos,
                    yaw: input.yaw,
                    flags: input.flags,
                    speed: input.speed,
                    vy: input.vy,
                    seq: input.seq,
                }));
            }
        }
    }
}

/// The screen follows the run: the cards when the local hero has some, the results at the end.
pub(super) fn follow_phase(
    phase: Res<State<Phase>>,
    mut next: ResMut<NextState<Phase>>,
    session: Res<Session>,
    mut offer: ResMut<Offer>,
    pending: Res<cards::PendingPick>,
) {
    if !session.running() {
        return;
    }
    if offer.choices() != session.offer.as_slice() && !pending.busy() {
        offer.set(session.offer.clone());
    }
    match phase.get() {
        Phase::Playing | Phase::Paused if session.over => next.set(Phase::GameOver),
        Phase::Playing if !session.offer.is_empty() => next.set(Phase::LevelUp),
        Phase::LevelUp if session.offer.is_empty() && !pending.busy() && !session.picked => {
            next.set(if session.over {
                Phase::GameOver
            } else {
                Phase::Playing
            });
        }
        _ => {}
    }
}
