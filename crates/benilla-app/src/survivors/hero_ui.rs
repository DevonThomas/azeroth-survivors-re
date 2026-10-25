//! The Hero screen, opened from the pick screen: the hero's look (the character-create dials), its
//! gear (worn slots and the collection), its class's three talent trees, the account's perks and
//! achievements. Also the level-up screen's card tricks (reroll, banish, pass for gold) and the
//! lines over the field: loot as it drops, and the run's payout on the results screen.

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use survivors_progress::gear::{ItemKey, Slot, ARMOUR_SLOTS};
use survivors_progress::{perks, ACHIEVEMENTS, PERKS};

use super::coop::Net;
use super::hud::{self, HudFont, MenuRoot, SpellIcon};
use super::menu::MenuBuilt;
use super::progress::{self, GearArt, Progress, TalentBook};
use super::session::Session;
use super::{HeroLoadout, Phase, Run, Selection};
use crate::glue::GlueClicks;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(super) enum Tab {
    #[default]
    Look,
    Gear,
    Talents,
    Perks,
    Achievements,
}

#[derive(Resource, Default)]
pub(super) struct HeroScreen {
    open: bool,
    tab: Tab,
    /// Bumped by every change the open tab shows.
    rev: u32,
    built: Option<(Tab, u32, Option<usize>)>,
}

impl HeroScreen {
    /// Open on `tab`, or close with `None` (the auto-runner's tour).
    pub(super) fn show(&mut self, tab: Option<Tab>) {
        self.open = tab.is_some();
        if let Some(t) = tab {
            self.tab = t;
        }
        self.rev += 1;
    }
}

pub(super) const TABS: [Tab; 5] = [Tab::Look, Tab::Gear, Tab::Talents, Tab::Perks, Tab::Achievements];

#[derive(Component, Clone, PartialEq, Debug)]
pub(super) enum HeroAction {
    Open,
    Close,
    Tab(Tab),
    Dial(usize, i8),
    Equip(ItemKey),
    Unequip(Slot),
    Learn { id: u32, row: u8, max_rank: u8, tree: usize },
    ResetTalents,
    BuyPerk(&'static str),
    ResetPerks,
    Reroll,
    Banish(usize),
    Skip,
}

#[derive(Component)]
pub(super) struct HeroPanel;
#[derive(Component)]
pub(super) struct HeroContent;
#[derive(Component)]
pub(super) struct HeroBuilt;
#[derive(Component)]
pub(super) struct Overlay;
#[derive(Component)]
pub(super) struct TrickRow;
#[derive(Component)]
pub(super) struct TrickText;

/// An inventory icon path to load into this node.
#[derive(Component)]
pub(super) struct ItemIcon(pub String);
#[derive(Component)]
pub(super) struct ItemIconDone;

const PANEL: Color = Color::srgba(0.03, 0.03, 0.05, 0.98);
const EDGE: Color = Color::srgb(0.55, 0.45, 0.25);
const BTN: Color = Color::srgba(0.18, 0.06, 0.04, 0.95);
const DIM: Color = Color::srgb(0.6, 0.6, 0.6);
const GOOD: Color = Color::srgb(0.3, 1.0, 0.3);

fn quality(q: u8) -> Color {
    match q {
        3 => Color::srgb(0.0, 0.44, 0.87),
        4 => Color::srgb(0.64, 0.21, 0.93),
        _ => Color::WHITE,
    }
}

fn button(p: &mut ChildSpawnerCommands, f: &Handle<Font>, action: HeroAction, label: &str, w: f32) {
    p.spawn((
        action,
        Button,
        Interaction::default(),
        Node {
            width: Val::Px(w),
            height: Val::Px(26.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(BTN),
        BorderColor::all(EDGE),
    ))
    .with_children(|b| {
        b.spawn(hud::text_bundle(f, label, 13.0, Color::WHITE));
    });
}

fn icon_node(size: f32) -> Node {
    Node {
        width: Val::Px(size),
        height: Val::Px(size),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

/// The Hero button on the pick screen, and the panel it opens; the field overlay.
pub(super) fn build(
    mut commands: Commands,
    roots: Query<Entity, (With<MenuRoot>, With<MenuBuilt>, Without<HeroBuilt>)>,
    font: Res<HudFont>,
) {
    let Ok(root) = roots.single() else { return };
    let f = &font.0;
    commands.entity(root).insert(HeroBuilt).with_children(|ui| {
        ui.spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(250.0),
            bottom: Val::Px(40.0),
            ..default()
        })
        .with_children(|b| button(b, f, HeroAction::Open, "Hero: gear, talents, perks", 230.0));
        ui.spawn((
            HeroPanel,
            Node {
                position_type: PositionType::Absolute,
                // Between the class tower and the zone list, under the co-op panel.
                left: Val::Px(244.0),
                top: Val::Px(150.0),
                width: Val::Px(860.0),
                height: Val::Px(515.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(2.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(EDGE),
            GlobalZIndex(900),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    for (t, label) in [
                        (Tab::Look, "Appearance"),
                        (Tab::Gear, "Gear"),
                        (Tab::Talents, "Talents"),
                        (Tab::Perks, "Perks"),
                        (Tab::Achievements, "Achievements"),
                    ] {
                        button(row, f, HeroAction::Tab(t), label, 128.0);
                    }
                    button(row, f, HeroAction::Close, "Close", 90.0);
                });
            panel.spawn((
                HeroContent,
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
            ));
        });
    });
    // Over the field: loot and achievements as they come, the payout on the results screen, and
    // the card tricks under the level-up cards.
    commands.spawn((
        Overlay,
        hud::centered_bundle(f, "", 16.0, Color::srgb(1.0, 0.85, 0.3)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            width: Val::Percent(100.0),
            ..default()
        },
        // Over the results screen's own dimming.
        GlobalZIndex(1000),
    ));
    commands
        .spawn((
            TrickRow,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(70.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(16),
        ))
        .with_children(|col| {
            col.spawn(Node {
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                button(row, f, HeroAction::Reroll, "Reroll (R)", 110.0);
                for i in 0..3 {
                    button(row, f, HeroAction::Banish(i), &format!("Banish {}", i + 1), 90.0);
                }
                button(row, f, HeroAction::Skip, "Take gold (S)", 120.0);
            });
            col.spawn((TrickText, hud::text_bundle(f, "", 13.0, DIM)));
        });
}

/// The panel shows only on the pick screen, while open.
pub(super) fn tabs(
    phase: Res<State<Phase>>,
    mut screen: ResMut<HeroScreen>,
    mut panels: Query<&mut Node, With<HeroPanel>>,
) {
    if *phase.get() != Phase::Menu && screen.open {
        screen.open = false;
    }
    for mut n in &mut panels {
        let want = if screen.open { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
}

/// Rebuild the open tab when it or what it shows changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild(
    mut commands: Commands,
    mut screen: ResMut<HeroScreen>,
    contents: Query<Entity, With<HeroContent>>,
    font: Res<HudFont>,
    selection: Res<Selection>,
    progress: Res<Progress>,
    art: Res<GearArt>,
    book: Res<TalentBook>,
    hero: Res<HeroLoadout>,
    char_create: Option<Res<crate::entities::CharCreate>>,
) {
    if !screen.open {
        return;
    }
    let key = (screen.tab, screen.rev, selection.class);
    if screen.built == Some(key) {
        return;
    }
    let Ok(content) = contents.single() else { return };
    screen.built = Some(key);
    let f = &font.0;
    let Some(class) = selection.class_def() else { return };
    let cid = class.id;
    let acc = &progress.account;
    commands.entity(content).despawn_related::<Children>();
    commands.entity(content).with_children(|c| match screen.tab {
        Tab::Look => {
            let l = hero.loadout.look;
            let ranges = char_create
                .as_deref()
                .and_then(|cc| cc.0.ranges(l.race, l.sex));
            c.spawn(hud::text_bundle(
                f,
                format!(
                    "{} {}: the race and gender come from the pick screen; the rest is yours.",
                    crate::char_select::race_name(l.race),
                    class.name
                ),
                15.0,
                Color::WHITE,
            ));
            let dials = [
                ("Skin colour", l.skin, ranges.map(|r| r.skin)),
                ("Face", l.face, ranges.map(|r| r.face)),
                ("Hair style", l.hair, ranges.map(|r| r.hair_style)),
                ("Hair colour", l.hair_color, ranges.map(|r| r.hair_color)),
                (
                    if l.sex == 0 { "Facial hair" } else { "Features" },
                    l.facial,
                    ranges.map(|r| r.facial_hair),
                ),
            ];
            for (i, (name, v, n)) in dials.into_iter().enumerate() {
                c.spawn(Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        hud::text_bundle(f, name, 15.0, Color::WHITE),
                        Node {
                            width: Val::Px(140.0),
                            ..default()
                        },
                    ));
                    button(row, f, HeroAction::Dial(i, -1), "<", 36.0);
                    let shown = match n {
                        Some(n) => format!("{} / {}", v + 1, n.max(1)),
                        None => format!("{}", v + 1),
                    };
                    row.spawn((
                        hud::text_bundle(f, shown, 15.0, Color::WHITE),
                        Node {
                            width: Val::Px(70.0),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                    ));
                    button(row, f, HeroAction::Dial(i, 1), ">", 36.0);
                });
            }
        }
        Tab::Gear => {
            let worn = acc.classes.get(&cid).map(|c| c.equipped.clone()).unwrap_or_default();
            let s = hero.loadout.stats;
            c.spawn(hud::text_bundle(
                f,
                format!(
                    "Bosses drop your class's Classic armour sets: Dungeon 1 in the early zones, then Tier 1, 2 and 3. Click a piece to wear it, a slot to take it off.\nWorn and talents together: +{:.0}% damage, +{:.0} health, +{:.1}% crit, +{:.1}% haste, +{:.1} armour.   Gold banked: {}",
                    s.might * 100.0,
                    s.health,
                    s.crit * 100.0,
                    s.haste * 100.0,
                    s.armor,
                    acc.gold
                ),
                13.0,
                DIM,
            ));
            c.spawn(Node {
                column_gap: Val::Px(18.0),
                ..default()
            })
            .with_children(|cols| {
                cols.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Px(330.0),
                    ..default()
                })
                .with_children(|slots| {
                    for slot in ARMOUR_SLOTS {
                        let item = worn.get(&slot.index()).copied();
                        slots
                            .spawn((
                                HeroAction::Unequip(slot),
                                Button,
                                Interaction::default(),
                                Node {
                                    column_gap: Val::Px(8.0),
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                            ))
                            .with_children(|row| {
                                let mut n = row.spawn((
                                    icon_node(34.0),
                                    ImageNode::default(),
                                    BorderColor::all(EDGE),
                                ));
                                if let Some(icon) = item.and_then(|i| art.icon(i)) {
                                    n.insert(ItemIcon(icon.to_string()));
                                }
                                let (text, color) = match item {
                                    Some(i) => (i.name(), quality(i.tier.quality())),
                                    None => (format!("{}: empty", slot.name()), DIM),
                                };
                                row.spawn(hud::text_bundle(f, text, 13.0, color));
                            });
                    }
                });
                cols.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    width: Val::Px(520.0),
                    align_content: AlignContent::FlexStart,
                    ..default()
                })
                .with_children(|grid| {
                    let owned = progress::owned(acc, cid);
                    if owned.is_empty() {
                        grid.spawn(hud::text_bundle(
                            f,
                            "No gear yet: defeat a boss to earn a piece.",
                            14.0,
                            DIM,
                        ));
                    }
                    for item in owned {
                        let mut b = grid.spawn((
                            HeroAction::Equip(item),
                            Button,
                            Interaction::default(),
                            icon_node(40.0),
                            ImageNode::default(),
                            BorderColor::all(quality(item.tier.quality())),
                        ));
                        if let Some(icon) = art.icon(item) {
                            b.insert(ItemIcon(icon.to_string()));
                        }
                    }
                });
            });
        }
        Tab::Talents => {
            let cp = acc.classes.get(&cid).cloned().unwrap_or_default();
            let total = cp.talent_points();
            let next = survivors_progress::talents::xp_for_point(total);
            c.spawn(Node {
                column_gap: Val::Px(12.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn(hud::text_bundle(
                    f,
                    format!(
                        "Talent points: {} free of {}   (talent experience {} / {} for the next; runs earn it)",
                        cp.talent_points_free(),
                        total,
                        cp.talent_xp,
                        next
                    ),
                    14.0,
                    Color::WHITE,
                ));
                button(row, f, HeroAction::ResetTalents, "Reset", 80.0);
            });
            let trees = book.trees.get(&cid);
            c.spawn(Node {
                column_gap: Val::Px(16.0),
                ..default()
            })
            .with_children(|cols| {
                let Some(trees) = trees else {
                    cols.spawn(hud::text_bundle(f, "Loading the talent trees…", 14.0, DIM));
                    return;
                };
                for (ti, tree) in trees.iter().enumerate() {
                    let spent = tree.spent(acc, cid);
                    cols.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        width: Val::Px(270.0),
                        ..default()
                    })
                    .with_children(|t| {
                        t.spawn(hud::text_bundle(
                            f,
                            format!("{}  ({spent})", tree.name),
                            15.0,
                            hud::GOLD_TONE,
                        ));
                        t.spawn(Node {
                            display: Display::Grid,
                            grid_template_columns: RepeatedGridTrack::px(4, 48.0),
                            grid_auto_rows: vec![GridTrack::px(50.0)],
                            column_gap: Val::Px(10.0),
                            ..default()
                        })
                        .with_children(|g| {
                            for (info, spell) in &tree.talents {
                                let rank = cp.talents.get(&info.id).copied().unwrap_or(0);
                                let open = survivors_progress::talents::row_open(info.row, spent);
                                let color = if rank == info.max_rank {
                                    hud::GOLD_TONE
                                } else if rank > 0 {
                                    GOOD
                                } else if open {
                                    Color::WHITE
                                } else {
                                    Color::srgb(0.35, 0.35, 0.35)
                                };
                                g.spawn((
                                    HeroAction::Learn {
                                        id: info.id,
                                        row: info.row,
                                        max_rank: info.max_rank,
                                        tree: ti,
                                    },
                                    Button,
                                    Interaction::default(),
                                    Node {
                                        grid_row: GridPlacement::start(i16::from(info.row) + 1),
                                        grid_column: GridPlacement::start(i16::from(info.col) + 1),
                                        flex_direction: FlexDirection::Column,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                ))
                                .with_children(|cell| {
                                    cell.spawn((
                                        icon_node(32.0),
                                        ImageNode::default(),
                                        SpellIcon(*spell),
                                        BorderColor::all(color),
                                    ));
                                    cell.spawn(hud::text_bundle(
                                        f,
                                        format!("{rank}/{}", info.max_rank),
                                        11.0,
                                        color,
                                    ));
                                });
                            }
                        });
                    });
                }
            });
            c.spawn(hud::text_bundle(
                f,
                "Your class's WoW Classic talent trees. Each tier opens after 5 points in its tree. A talent named for one of your spells empowers it; one that teaches a spell starts every run with it.",
                12.0,
                DIM,
            ));
        }
        Tab::Perks => {
            c.spawn(Node {
                column_gap: Val::Px(12.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn(hud::text_bundle(
                    f,
                    format!(
                        "Perk points: {} free of {} (one for each achievement). Perks are the account's: every hero has them.",
                        acc.perk_points_free(),
                        acc.perk_points()
                    ),
                    14.0,
                    Color::WHITE,
                ));
                button(row, f, HeroAction::ResetPerks, "Reset", 80.0);
            });
            for p in PERKS {
                let rank = acc.perks.get(p.key).copied().unwrap_or(0);
                c.spawn(Node {
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((icon_node(30.0), ImageNode::default(), SpellIcon(p.icon_spell)));
                    row.spawn((
                        hud::text_bundle(
                            f,
                            format!("{}  {rank}/{}   ({} pt)", p.name, p.ranks, p.cost),
                            14.0,
                            if rank > 0 { GOOD } else { Color::WHITE },
                        ),
                        Node {
                            width: Val::Px(240.0),
                            ..default()
                        },
                    ));
                    row.spawn((
                        hud::text_bundle(f, p.desc, 13.0, DIM),
                        Node {
                            width: Val::Px(470.0),
                            ..default()
                        },
                    ));
                    if rank < p.ranks {
                        button(row, f, HeroAction::BuyPerk(p.key), "Buy", 60.0);
                    }
                });
            }
        }
        Tab::Achievements => {
            c.spawn(hud::text_bundle(
                f,
                format!(
                    "{} of {} earned. Each is a perk point.   Runs {}, wins {}, kills {}, bosses {}.",
                    acc.achievements.len(),
                    ACHIEVEMENTS.len(),
                    acc.lifetime.runs,
                    acc.lifetime.wins,
                    acc.lifetime.kills,
                    acc.lifetime.bosses
                ),
                14.0,
                Color::WHITE,
            ));
            c.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(10.0),
                row_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|grid| {
                for a in ACHIEVEMENTS.iter() {
                    let done = acc.achievements.contains(&a.key);
                    grid.spawn(Node {
                        width: Val::Px(440.0),
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((icon_node(24.0), ImageNode::default(), SpellIcon(a.icon_spell)));
                        row.spawn(hud::text_bundle(
                            f,
                            format!("{}{}: {}", if done { "✓ " } else { "" }, a.name, a.desc),
                            12.0,
                            if done { GOOD } else { DIM },
                        ));
                    });
                }
            });
        }
    });
}

/// The panel's and the level-up screen's buttons.
#[allow(clippy::too_many_arguments)]
pub(super) fn clicks(
    clicks: Res<GlueClicks>,
    actions: Query<(Entity, &HeroAction)>,
    mut screen: ResMut<HeroScreen>,
    mut progress: ResMut<Progress>,
    selection: Res<Selection>,
    hero: Res<HeroLoadout>,
    book: Res<TalentBook>,
    char_create: Option<Res<crate::entities::CharCreate>>,
    mut session: ResMut<Session>,
    mut net: ResMut<Net>,
) {
    let Some((_, action)) = actions.iter().find(|(e, _)| clicks.hit(*e)) else {
        return;
    };
    let cid = selection.class_def().map(|c| c.id);
    match action.clone() {
        HeroAction::Open => {
            screen.open = true;
            screen.rev += 1;
        }
        HeroAction::Close => screen.open = false,
        HeroAction::Tab(t) => screen.tab = t,
        HeroAction::Dial(i, d) => {
            let Some(cid) = cid else { return };
            let mut l = hero.loadout.look;
            let r = char_create.as_deref().and_then(|cc| cc.0.ranges(l.race, l.sex));
            let (v, n) = match i {
                0 => (&mut l.skin, r.map(|r| r.skin)),
                1 => (&mut l.face, r.map(|r| r.face)),
                2 => (&mut l.hair, r.map(|r| r.hair_style)),
                3 => (&mut l.hair_color, r.map(|r| r.hair_color)),
                _ => (&mut l.facial, r.map(|r| r.facial_hair)),
            };
            let n = i16::from(n.unwrap_or(1).max(1));
            *v = (i16::from(*v) + i16::from(d)).rem_euclid(n) as u8;
            progress.account.class(cid).look = l;
            progress.changed();
        }
        HeroAction::Equip(item) => {
            progress.account.equip(item);
            progress.changed();
        }
        HeroAction::Unequip(slot) => {
            let Some(cid) = cid else { return };
            progress.account.unequip(cid, slot);
            progress.changed();
        }
        HeroAction::Learn { id, row, max_rank, tree } => {
            let Some(cid) = cid else { return };
            let spent = book
                .trees
                .get(&cid)
                .and_then(|t| t.get(tree))
                .map_or(0, |t| t.spent(&progress.account, cid));
            if progress.account.learn_talent(cid, id, row, max_rank, spent) {
                progress.changed();
            }
        }
        HeroAction::ResetTalents => {
            let Some(cid) = cid else { return };
            progress.account.reset_talents(cid);
            progress.changed();
        }
        HeroAction::BuyPerk(key) => {
            if progress.account.buy_perk(key) {
                progress.changed();
            }
        }
        HeroAction::ResetPerks => {
            progress.account.reset_perks();
            progress.changed();
        }
        HeroAction::Reroll => session.reroll(&mut net),
        HeroAction::Banish(i) => session.banish(i, &mut net),
        HeroAction::Skip => session.skip(&mut net),
    }
    screen.rev += 1;
}

/// Keys for the card tricks on the level-up screen: R reroll, Shift+1/2/3 banish, S take gold.
pub(super) fn card_tricks(
    phase: Res<State<Phase>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<Session>,
    mut net: ResMut<Net>,
) {
    if *phase.get() != Phase::LevelUp {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        session.reroll(&mut net);
    }
    if keys.just_pressed(KeyCode::KeyS) {
        session.skip(&mut net);
    }
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3].into_iter().enumerate() {
            if keys.just_pressed(k) {
                session.banish(i, &mut net);
            }
        }
    }
}

/// Load each inventory icon into its node.
pub(super) fn fill_item_icons(
    mut commands: Commands,
    mut slots: Query<(Entity, &mut ImageNode, &ItemIcon), Without<ItemIconDone>>,
    assets: Option<ResMut<benilla_assets::WorldAssets>>,
    mut images: ResMut<Assets<Image>>,
) {
    if slots.is_empty() {
        return;
    }
    let Some(mut assets) = assets else { return };
    for (e, mut img, icon) in &mut slots {
        if let Some(h) = assets.sprite_texture(&format!("{}.blp", icon.0), &mut images) {
            img.image = h;
        }
        commands.entity(e).insert(ItemIconDone);
    }
}

/// The lines over the field, and the card tricks' row.
#[allow(clippy::type_complexity)]
pub(super) fn refresh_overlay(
    phase: Res<State<Phase>>,
    progress: Res<Progress>,
    run: Res<Run>,
    mut overlay: Query<&mut Text, (With<Overlay>, Without<TrickText>)>,
    mut row: Query<&mut Node, With<TrickRow>>,
    mut tricks: Query<&mut Text, (With<TrickText>, Without<Overlay>)>,
) {
    let p = *phase.get();
    let text = match p {
        Phase::GameOver => progress.payout.as_ref().map_or(String::new(), |(pay, loot)| {
            let mut s = format!(
                "+{} talent experience{}   +{} gold banked",
                pay.talent_xp,
                if pay.new_points > 0 {
                    format!(" ({} new talent point{})", pay.new_points, if pay.new_points > 1 { "s" } else { "" })
                } else {
                    String::new()
                },
                pay.gold
            );
            for item in loot {
                s.push_str(&format!("\nLoot: {}", item.name()));
            }
            for a in &pay.new_achievements {
                s.push_str(&format!("\nAchievement: {a}"));
            }
            s
        }),
        Phase::Menu => String::new(),
        _ => progress
            .toasts
            .iter()
            .map(|t| t.0.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    };
    if let Ok(mut t) = overlay.single_mut() {
        if t.0 != text {
            t.0 = text;
        }
    }
    let show = p == Phase::LevelUp
        && (run.rerolls > 0 || run.banishes > 0 || run.loadout.skips);
    if let Ok(mut n) = row.single_mut() {
        let want = if show { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    if show {
        if let Ok(mut t) = tricks.single_mut() {
            let s = format!(
                "Rerolls left {} (R)   Banishes left {} (Shift+1/2/3)   {}",
                run.rerolls,
                run.banishes,
                if run.loadout.skips { "S: pass this hand for gold" } else { "" }
            );
            if t.0 != s {
                t.0 = s;
            }
        }
    }
    let _ = perks::PERKS.len();
}
