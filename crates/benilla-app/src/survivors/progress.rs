//! The player's account on this machine (`benilla-config/survivors-progress.json`), and what the
//! client adds to it: the art each set piece wears (found in this client's `ItemDisplayInfo` by the
//! set's texture family), each class's talent trees (this client's Talent.dbc), the loot a boss
//! drops, and the run's payout at the end.

use std::collections::HashMap;

use bevy::prelude::*;
use survivors_progress::account::Payout;
use survivors_progress::gear::{ItemKey, Slot, TIERS};
use survivors_progress::{Account, RunResult, TalentInfo};
use survivors_sim::{data, Appearance, Loadout, Rng};

use super::{Run, Selection};

fn progress_path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("survivors-progress.json"))
}

#[derive(Resource)]
pub(super) struct Progress {
    pub account: Account,
    dirty: bool,
    /// The last run's payout and the loot it dropped, for the results screen.
    pub payout: Option<(Payout, Vec<ItemKey>)>,
    /// Loot dropped this run.
    pub loot: Vec<ItemKey>,
    /// Short lines to show over the field: loot, achievements.
    pub toasts: Vec<(String, f32)>,
}

impl Default for Progress {
    fn default() -> Self {
        let account = progress_path()
            .and_then(|p| crate::local_state::read_to_string(&p).ok())
            .and_then(|t| Account::from_json(&t))
            .unwrap_or(Account {
                version: Account::VERSION,
                ..Account::default()
            });
        Self {
            account,
            dirty: false,
            payout: None,
            loot: Vec::new(),
            toasts: Vec::new(),
        }
    }
}

impl Progress {
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    pub fn toast(&mut self, line: impl Into<String>) {
        self.toasts.push((line.into(), 5.0));
    }
}

/// Write the account when it changed.
pub(super) fn save(mut progress: ResMut<Progress>) {
    if !progress.dirty {
        return;
    }
    progress.dirty = false;
    if let Some(p) = progress_path() {
        let _ = crate::local_state::write_atomic(&p, &progress.account.to_json());
    }
}

/// The art every set piece wears, and its icon.
#[derive(Resource, Default)]
pub(super) struct GearArt {
    pub pieces: HashMap<ItemKey, (u32, Option<String>)>,
    built: bool,
}

impl GearArt {
    pub fn display(&self, item: ItemKey) -> Option<u32> {
        self.pieces.get(&item).map(|p| p.0)
    }
    pub fn icon(&self, item: ItemKey) -> Option<&str> {
        self.pieces.get(&item).and_then(|p| p.1.as_deref())
    }
}

/// Which slot an inventory icon is for.
fn slot_of_icon(icon: &str) -> Option<Slot> {
    let i = icon.to_ascii_lowercase();
    let name = i.rsplit('\\').next().unwrap_or(&i);
    Some(if name.starts_with("inv_helmet") || name.starts_with("inv_crown") {
        Slot::Head
    } else if name.starts_with("inv_shoulder") {
        Slot::Shoulders
    } else if name.starts_with("inv_chest") {
        Slot::Chest
    } else if name.starts_with("inv_gauntlets") {
        Slot::Hands
    } else if name.starts_with("inv_boots") {
        Slot::Feet
    } else if name.starts_with("inv_bracer") {
        Slot::Wrists
    } else if name.starts_with("inv_belt") {
        Slot::Waist
    } else if name.starts_with("inv_pants") {
        Slot::Legs
    } else {
        return None;
    })
}

/// Find each class's four sets in this client's art: a display whose every model and texture is
/// of the set's family, its slot told by its inventory icon, the lowest id for each slot.
pub(super) fn build_gear_art(
    displays: Option<Res<crate::entities::ItemDisplays>>,
    mut art: ResMut<GearArt>,
) {
    if art.built {
        return;
    }
    let Some(displays) = displays else { return };
    art.built = true;
    let mut found: HashMap<ItemKey, (u32, Option<String>)> = HashMap::new();
    for (id, d) in displays.catalog.iter() {
        let Some(icon) = d.icon.as_deref() else { continue };
        let Some(slot) = slot_of_icon(icon) else { continue };
        let names: Vec<String> = d
            .model
            .iter()
            .chain(d.model_texture.iter())
            .chain(d.region_textures.iter())
            .flatten()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        if names.is_empty() {
            continue;
        }
        for class in data::CLASSES.iter().map(|c| c.id) {
            for tier in TIERS {
                let Some(family) = tier.art(class) else { continue };
                if names.iter().all(|n| n.contains(&family)) {
                    let key = ItemKey { class, tier, slot };
                    let better = found.get(&key).is_none_or(|(old, _)| id < *old);
                    if better {
                        found.insert(key, (id, Some(icon.to_string())));
                    }
                }
            }
        }
    }
    // A piece the family has no art of its own for borrows the closest: a display of the slot
    // where most of the names are the family's.
    let mut loose: HashMap<ItemKey, (usize, u32, Option<String>)> = HashMap::new();
    for (id, d) in displays.catalog.iter() {
        let Some(icon) = d.icon.as_deref() else { continue };
        let Some(slot) = slot_of_icon(icon) else { continue };
        let names: Vec<String> = d
            .model
            .iter()
            .chain(d.model_texture.iter())
            .chain(d.region_textures.iter())
            .flatten()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        for class in data::CLASSES.iter().map(|c| c.id) {
            for tier in TIERS {
                let key = ItemKey { class, tier, slot };
                if found.contains_key(&key) {
                    continue;
                }
                let Some(family) = tier.art(class) else { continue };
                let hits = names.iter().filter(|n| n.contains(&family)).count();
                if hits > 0 && loose.get(&key).is_none_or(|(h, old, _)| hits > *h || (hits == *h && id < *old)) {
                    loose.insert(key, (hits, id, Some(icon.to_string())));
                }
            }
        }
    }
    for (key, (_, id, icon)) in loose {
        found.insert(key, (id, icon));
    }
    info!("survivors: gear art for {} set pieces", found.len());
    art.pieces = found;
}

/// One talent tree as the client's tables give it.
pub(super) struct Tree {
    pub name: String,
    pub talents: Vec<(TalentInfo, u32)>,
}

impl Tree {
    /// Points spent in this tree.
    pub fn spent(&self, account: &Account, class: u8) -> u32 {
        account.classes.get(&class).map_or(0, |cp| {
            self.talents
                .iter()
                .map(|(t, _)| u32::from(cp.talents.get(&t.id).copied().unwrap_or(0)))
                .sum()
        })
    }
}

/// Each class's three talent trees: names and spells for icons from the client's Spell.dbc.
#[derive(Resource, Default)]
pub(super) struct TalentBook {
    pub trees: HashMap<u8, Vec<Tree>>,
    built: bool,
}

impl TalentBook {
    pub fn info(&self, class: u8, id: u32) -> Option<TalentInfo> {
        self.trees
            .get(&class)?
            .iter()
            .flat_map(|t| &t.talents)
            .find(|(t, _)| t.id == id)
            .map(|(t, _)| t.clone())
    }
}

pub(super) fn build_talent_book(
    talents: Option<Res<crate::ui_talent::Talents>>,
    spells: Option<Res<crate::ui_action::Spells>>,
    mut book: ResMut<TalentBook>,
) {
    if book.built {
        return;
    }
    let (Some(talents), Some(spells)) = (talents, spells) else {
        return;
    };
    book.built = true;
    for class in data::CLASSES {
        // Every race the class can be shares its trees; ask with any (511 races in 1.12's data).
        let tabs = (1..=8u8)
            .map(|race| talents.catalog.tabs_for_class(race, class.id))
            .find(|t| !t.is_empty())
            .unwrap_or_default();
        let trees = tabs
            .iter()
            .map(|tab| Tree {
                name: tab.name.clone(),
                talents: talents
                    .catalog
                    .talents_in_tab(tab.id)
                    .iter()
                    .filter_map(|t| {
                        let spell = t.ranks[0];
                        let name = spells.catalog.get(spell)?.name.clone();
                        Some((
                            TalentInfo {
                                id: t.id,
                                name,
                                tree: tab.name.clone(),
                                row: t.row as u8,
                                col: t.col as u8,
                                max_rank: t.max_rank() as u8,
                                teaches: false,
                            },
                            spell,
                        ))
                    })
                    .collect(),
            })
            .collect();
        book.trees.insert(class.id, trees);
    }
}

/// The hero's look for `class`: race and sex as picked at the menu, the rest as saved.
pub(super) fn look_for(account: &Account, class: u8, race: u8, sex: u8) -> Appearance {
    let mut look = account.classes.get(&class).map(|c| c.look).unwrap_or_default();
    if look.race != race || look.sex != sex {
        look = Appearance {
            race,
            sex,
            ..Appearance::default()
        };
    }
    look
}

/// The loadout the account brings a run as `class`.
pub(super) fn loadout_for(
    account: &Account,
    class: u8,
    race: u8,
    sex: u8,
    book: &TalentBook,
    art: &GearArt,
) -> Loadout {
    let mut l = survivors_progress::loadout(
        account,
        class,
        &|id| book.info(class, id),
        &|item| art.display(item),
    );
    l.look = look_for(account, class, race, sex);
    l
}

/// The worn gear's art for `class`, in loadout order.
pub(super) fn worn_gear(account: &Account, class: u8, art: &GearArt) -> [u32; survivors_sim::GEAR_SLOTS] {
    let mut gear = [0; survivors_sim::GEAR_SLOTS];
    if let Some(cp) = account.classes.get(&class) {
        for item in cp.equipped.values() {
            if let Some(d) = art.display(*item) {
                gear[item.slot.gear_index()] = d;
            }
        }
    }
    gear
}

/// A boss fell: the local hero's piece of that zone's set, into the collection.
#[derive(Message, Clone, Copy)]
pub(super) struct BossLoot {
    pub map: u16,
}

pub(super) fn grant_loot(
    mut drops: MessageReader<BossLoot>,
    run: Res<Run>,
    mut progress: ResMut<Progress>,
    mut rng: ResMut<super::Rng>,
) {
    for d in drops.read() {
        let class = run.class.id;
        let mut dice = Rng::seeded(rng.next_u64());
        let item = progress
            .account
            .roll_drop(class, usize::from(d.map), &mut dice);
        progress.loot.push(item);
        let line = format!("Loot: {}", item.name());
        progress.toast(line);
        progress.changed();
    }
}

/// The run ended: pay it out to the account.
pub(super) fn bank_run(
    run: Res<Run>,
    selection: Res<Selection>,
    mut progress: ResMut<Progress>,
) {
    let result = RunResult {
        class: run.class.id,
        map: selection.stage as u16,
        won: run.won,
        level: run.level,
        kills: run.kills,
        bosses: run.bosses,
        gold: run.gold,
        seconds: run.elapsed,
    };
    let payout = progress.account.finish_run(&result);
    for a in &payout.new_achievements {
        let line = format!("Achievement: {a}");
        progress.toast(line);
    }
    let loot = std::mem::take(&mut progress.loot);
    progress.payout = Some((payout, loot));
    progress.changed();
}

/// Toasts count down.
pub(super) fn tick_toasts(time: Res<Time<Real>>, mut progress: ResMut<Progress>) {
    if progress.toasts.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    for t in &mut progress.toasts {
        t.1 -= dt;
    }
    progress.toasts.retain(|t| t.1 > 0.0);
}

/// Every slot's piece of every set `class` owns, by tier then slot.
pub(super) fn owned(account: &Account, class: u8) -> Vec<ItemKey> {
    let mut v: Vec<ItemKey> = account
        .collection
        .iter()
        .copied()
        .filter(|i| i.class == class)
        .collect();
    v.sort();
    v
}
