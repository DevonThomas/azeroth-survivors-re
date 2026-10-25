//! Warcraft Survivors: a top-down survival mode on the benilla world, started by
//! [`crate::run_survivors`]. There is no server: the hero, the enemies and the spells are local
//! entities dressed by the same attach, animation and spell-visual systems a streamed unit gets,
//! on a battleground picked at the menu ([`data::MAPS`]), with the player controller and FrameXML
//! off ([`crate::run_mode::CaptureMode`]).

mod auto;
mod cards;
mod coop;
mod grade;
mod hero;
mod hero_ui;
mod hud;
mod menu;
mod minimap;
mod mobility;
mod player;
mod progress;
mod session;
mod units;
mod view;

use benilla_world::schedule::WorldStage;
use bevy::prelude::*;

use survivors_sim::data;
pub(super) use survivors_sim::{Owned, Stats};

/// Set by [`crate::run_survivors`] before the app builds. wasm32 has no process environment, so
/// the flag lives here; `WOW_SURVIVORS` (`?survivors=1` / `__wenilla_env.survivors` on the web)
/// still turns it on from outside.
pub(crate) static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn active() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
        || crate::webenv::var("WOW_SURVIVORS").is_some()
}

pub(crate) struct SurvivorsPlugin;

impl Plugin for SurvivorsPlugin {
    fn build(&self, app: &mut App) {
        // Present before `OnEnter(InWorld)`, where the UI lifecycle decides not to load FrameXML.
        app.insert_resource(crate::run_mode::CaptureMode)
            .add_plugins(grade::plugin)
            .init_state::<Phase>()
            .init_resource::<Rng>()
            .init_resource::<Selection>()
            .init_resource::<Run>()
            .init_resource::<hero::CamRig>()
            .init_resource::<hero::AimPoint>()
            .init_resource::<hud::Offer>()
            .init_resource::<cards::PendingPick>()
            .init_resource::<session::Session>()
            .init_resource::<view::Ids>()
            .insert_resource(coop::Net::from_env())
            .init_resource::<progress::Progress>()
            .init_resource::<progress::GearArt>()
            .init_resource::<progress::TalentBook>()
            .init_resource::<HeroLoadout>()
            .init_resource::<hero_ui::HeroScreen>()
            .add_message::<progress::BossLoot>()
            .add_systems(
                Update,
                (
                    progress::build_gear_art,
                    progress::build_talent_book,
                    refresh_loadout,
                )
                    .chain()
                    .before(hud::menu_preview),
            )
            .add_systems(
                Update,
                (
                    progress::grant_loot,
                    progress::tick_toasts,
                    hero_ui::build,
                    hero_ui::tabs,
                    hero_ui::rebuild,
                    hero_ui::clicks,
                    hero_ui::card_tricks,
                    hero_ui::fill_item_icons,
                    hero_ui::refresh_overlay,
                    hero_ui::tooltip,
                    progress::save,
                )
                    .chain()
                    .after(view::play_events),
            )
            .add_systems(OnEnter(Phase::GameOver), progress::bank_run)
            .add_plugins(bevy::ui_render::UiMaterialPlugin::<hud::SweepUiMaterial>::default())
            .add_systems(Startup, hud::setup_hud)
            .add_systems(Update, hero::daylight)
            // The server first: what it said this frame lands before anything reads the session.
            .add_systems(Update, coop::pump.before(WorldStage::Input))
            .add_systems(
                Update,
                (
                    hero::aim_cursor,
                    mobility::cast_mobility,
                    hero::drive_hero,
                    hero::place_camera,
                )
                    .chain()
                    .in_set(WorldStage::Input),
            )
            .add_systems(
                Update,
                (
                    session::advance
                        // Entering a battleground raises its loading screen: the run waits
                        // behind it.
                        .run_if(|l: Res<crate::loading_screen::LoadingScreen>| !l.covering()),
                    view::sync,
                    view::play_events,
                    session::follow_phase,
                    coop::flush,
                )
                    .chain()
                    .after(WorldStage::Input),
            )
            .add_systems(
                Update,
                view::still_gems.after(WorldStage::Input),
            )
            .add_systems(
                Update,
                (
                    hero::follow_map,
                    hud::menu_preview,
                    hud::buttons,
                    hud::keys,
                    hud::refresh_menu,
                    menu::restore_pick,
                    menu::build_menu,
                    menu::menu_clicks,
                    menu::refresh_menu_ui.before(crate::glue::GlueVisuals),
                    cards::deal_cards,
                    cards::choose_card,
                    cards::animate_cards,
                    hud::refresh_hud,
                    hud::refresh_results,
                    hud::fill_icons,
                    hud::update_cooldown_sweeps,
                    hud::refresh_mobility,
                    hud::show_panels,
                )
                    .chain()
                    .after(session::follow_phase),
            )
            .add_systems(
                Update,
                (
                    coop::setup_party_frame,
                    coop::build_panel,
                    coop::panel_input,
                    coop::refresh_panel,
                    coop::refresh_party_frame,
                )
                    .chain()
                    .after(menu::menu_clicks)
                    .before(crate::glue::GlueVisuals),
            )
            .add_systems(
                Update,
                (
                    hud::apply_ui_scale,
                    hud::fill_art,
                    hud::xp_hover,
                    hud::icon_tooltip,
                    hud::hero_plate,
                    minimap::map_buttons,
                    minimap::zone_text,
                    minimap::compose,
                )
                    .chain()
                    .after(hud::show_panels),
            )
            .add_systems(OnEnter(Phase::Playing), unpause)
            .add_systems(OnEnter(Phase::LevelUp), pause)
            .add_systems(OnEnter(Phase::Paused), pause)
            .add_systems(OnEnter(Phase::GameOver), unpause)
            .add_systems(OnEnter(Phase::Menu), (unpause, reset_run));
        if let Some(auto) = auto::from_env() {
            app.insert_resource(auto)
                .add_systems(
                    Update,
                    auto::drive.after(hud::keys).before(hud::refresh_menu),
                )
                .add_systems(Update, auto::plan.before(WorldStage::Input));
        }
    }
}

/// The screen the mode is on.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum Phase {
    #[default]
    Menu,
    Playing,
    LevelUp,
    Paused,
    GameOver,
}

/// Everything spawned for a run, despawned when the next begins.
#[derive(Component)]
pub(super) struct RunEntity;

/// The class, look and battleground picked at the menu; the battleground holds for the run.
#[derive(Resource, Default)]
pub(super) struct Selection {
    pub class: Option<usize>,
    /// `(display, race, sex)`, one per race and sex the class's trainers come in.
    pub looks: Vec<(u32, u8, u8)>,
    pub look: Option<usize>,
    /// An index into [`data::MAPS`].
    pub map: usize,
    /// The battleground the world is standing in: the last one entered (or the saved pick at
    /// boot). Picking another at the menu changes only `map`, so the menu hero stays put and no
    /// loading screen rises until Enter World.
    pub stage: usize,
    /// Bumped on every change, so the menu rebuilds once.
    pub version: u32,
}

impl Selection {
    pub fn class_def(&self) -> Option<&'static data::ClassDef> {
        self.class.map(|i| &data::CLASSES[i])
    }

    pub fn map_def(&self) -> &'static data::MapDef {
        &data::MAPS[self.map.min(data::MAPS.len() - 1)]
    }

    pub fn stage_def(&self) -> &'static data::MapDef {
        &data::MAPS[self.stage.min(data::MAPS.len() - 1)]
    }
}

/// The local hero's run, mirrored every frame from the session: the HUD and the cards read it.
#[derive(Resource, Default, Deref, DerefMut)]
pub(super) struct Run(pub survivors_sim::Run);

/// What the account brings a run as the hero picked at the menu, kept current while the menu is
/// up; `version` moves when the look or the worn gear changes, so the menu hero is redressed.
#[derive(Resource, Default)]
pub(super) struct HeroLoadout {
    pub loadout: survivors_sim::Loadout,
    pub version: u32,
}

fn refresh_loadout(
    phase: Res<State<Phase>>,
    selection: Res<Selection>,
    progress: Res<progress::Progress>,
    book: Res<progress::TalentBook>,
    art: Res<progress::GearArt>,
    mut hero: ResMut<HeroLoadout>,
) {
    if *phase.get() != Phase::Menu {
        return;
    }
    let (Some(class), Some(&(_, race, sex))) = (
        selection.class_def(),
        selection.look.and_then(|i| selection.looks.get(i)),
    ) else {
        return;
    };
    let l = progress::loadout_for(&progress.account, class.id, race, sex, &book, &art);
    if l != hero.loadout {
        if l.look != hero.loadout.look || l.gear != hero.loadout.gear {
            hero.version += 1;
        }
        hero.loadout = l;
    }
}

/// The client's own dice: the menu's Randomize and the cosmetic rolls. The run's are the sim's.
#[derive(Resource, Deref, DerefMut)]
pub(super) struct Rng(survivors_sim::Rng);

impl Default for Rng {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self(survivors_sim::Rng::from_clock(seed))
    }
}

/// A solo run stops the clock for the menu and the cards; a co-op one never does, as the party
/// plays on (the server holds the field still while anyone picks).
fn pause(mut time: ResMut<Time<Virtual>>, session: Res<session::Session>) {
    if !session.online() {
        time.pause();
    }
}

fn unpause(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

/// Back at the menu: everything the last run spawned goes, and the run starts over.
fn reset_run(
    mut commands: Commands,
    spawned: Query<Entity, With<RunEntity>>,
    mut run: ResMut<Run>,
    mut session: ResMut<session::Session>,
    mut ids: ResMut<view::Ids>,
    mut selection: ResMut<Selection>,
    mut net: ResMut<coop::Net>,
) {
    // Out of a co-op run before its end: out of its room too, so the party is not left waiting.
    if session.online() && !session.over {
        net.leave_room();
    }
    for e in &spawned {
        commands.entity(e).despawn();
    }
    // Guids stay unique across runs: caches keyed by guid may outlive a despawn by a frame.
    let next_guid = run.next_guid;
    run.0 = survivors_sim::Run {
        next_guid,
        ..Default::default()
    };
    session.end();
    ids.clear();
    // The preview hero is respawned for the look still picked.
    selection.version += 1;
}
