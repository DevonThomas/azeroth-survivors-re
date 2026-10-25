//! Co-op: the connection to the dedicated server, the room panel on the pick screen, and the
//! party frame in the run.
//!
//! The server address and the player name come from `AZS_SERVER` / `AZS_NAME`, else the last
//! ones used (`benilla-config/survivors-coop.txt`), else `127.0.0.1:7777` and the OS user name.
//! Both can be typed in on the panel. The browser build has no UDP: there the panel says so.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use survivors_net::{ClientMsg, Lobby, Pick, ServerMsg};
use survivors_sim::Loadout;

use super::hud::{self, HudFont, HudRoot, MenuRoot};
use super::menu::MenuBuilt;
use super::session::Session;
use super::{data, Phase, Run, Selection};
use crate::glue::art::GlueArt;
use crate::glue::widgets::{glue_button, GlueBtnKind, GlueDisabled};
use crate::glue::GlueClicks;

fn coop_path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("survivors-coop.txt"))
}

/// What the panel's text entry is filling.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Field {
    Code,
    Server,
    Name,
}

#[derive(Resource)]
pub(super) struct Net {
    #[cfg(not(target_arch = "wasm32"))]
    client: Option<survivors_net::Client>,
    pub server: String,
    pub name: String,
    pub lobby: Option<Lobby>,
    /// The panel's status line.
    pub status: String,
    /// Sent once the connection is up.
    queued: Vec<ClientMsg>,
    /// The text entry open on the panel, and what is typed so far.
    pub entry: Option<(Field, String)>,
    /// The run the server started for this client, applied once the screen is back at the menu.
    started: Option<(u16, survivors_sim::Id)>,
    /// The pick last sent to the room.
    sent_pick: Option<Pick>,
}

impl Net {
    pub fn from_env() -> Self {
        let saved = coop_path()
            .and_then(|p| crate::local_state::read_to_string(&p).ok())
            .unwrap_or_default();
        let mut lines = saved.lines().map(str::trim);
        let saved_server = lines.next().filter(|s| !s.is_empty()).map(String::from);
        let saved_name = lines.next().filter(|s| !s.is_empty()).map(String::from);
        let env = |k: &str| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::env::var(k).ok().filter(|v| !v.trim().is_empty())
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = k;
                None::<String>
            }
        };
        let server = env("AZS_SERVER")
            .or(saved_server)
            .unwrap_or_else(|| format!("127.0.0.1:{}", survivors_net::DEFAULT_PORT));
        let name = env("AZS_NAME")
            .or(saved_name)
            .or_else(|| env("USERNAME"))
            .or_else(|| env("USER"))
            .unwrap_or_else(|| "Adventurer".into());
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            client: None,
            server,
            name: name.chars().take(16).collect(),
            lobby: None,
            status: String::new(),
            queued: Vec::new(),
            entry: None,
            started: None,
            sent_pick: None,
        }
    }

    fn save(&self) {
        if let Some(p) = coop_path() {
            let _ = crate::local_state::write_atomic(&p, &format!("{}\n{}\n", self.server, self.name));
        }
    }

    pub fn typing(&self) -> bool {
        self.entry.is_some()
    }

    pub fn in_room(&self) -> bool {
        self.lobby.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn my_id(&self) -> Option<u64> {
        self.client.as_ref().map(survivors_net::Client::id)
    }

    #[cfg(target_arch = "wasm32")]
    fn my_id(&self) -> Option<u64> {
        None
    }

    pub fn leader(&self) -> bool {
        match (&self.lobby, self.my_id()) {
            (Some(l), Some(me)) => l.leader == me,
            _ => false,
        }
    }

    /// Send now if connected; dropped otherwise (inputs, picks: the next one supersedes).
    pub fn send(&mut self, msg: &ClientMsg) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(c) = &mut self.client {
            c.send(msg);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = msg;
    }

    /// Quick Play with `pick`: into any public room with space, or a new one.
    pub fn quick(&mut self, pick: Pick, map: u16) {
        self.sent_pick = Some(pick.clone());
        self.request(ClientMsg::Quick { pick, map });
    }

    pub fn leave_room(&mut self) {
        self.send(&ClientMsg::Leave);
        self.lobby = None;
        self.sent_pick = None;
    }

    pub fn members(&self) -> usize {
        self.lobby.as_ref().map_or(0, |l| l.members.len())
    }

    /// Send once connected, connecting first if need be.
    fn request(&mut self, msg: ClientMsg) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.client.is_none() {
                match survivors_net::Client::connect(&self.server, &self.name) {
                    Ok(c) => {
                        self.status = format!("Connecting to {}…", self.server);
                        self.client = Some(c);
                    }
                    Err(e) => {
                        self.status = format!("Can't reach {}: {e}", self.server);
                        return;
                    }
                }
            }
            self.queued.push(msg);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = msg;
            self.status = "Co-op needs the desktop build.".into();
        }
    }

    fn disconnect(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(mut c) = self.client.take() {
            c.disconnect();
        }
        self.lobby = None;
        self.queued.clear();
        self.sent_pick = None;
    }
}

/// The hero picked at the menu, for the room.
pub(super) fn pick_of(net: &Net, selection: &Selection, loadout: &Loadout) -> Option<Pick> {
    let class = selection.class?;
    let &(display, race, sex) = selection.look.and_then(|i| selection.looks.get(i))?;
    let mut loadout = loadout.clone();
    loadout.look.race = race;
    loadout.look.sex = sex;
    Some(Pick {
        name: net.name.clone(),
        class: class as u8,
        race,
        sex,
        display,
        loadout,
    })
}

/// Enter World: in a room the leader starts the run for everyone (the others wait for it);
/// alone, a solo run starts here. `true` when a solo run began.
pub(super) fn enter_world(
    net: &mut Net,
    session: &mut Session,
    run: &mut Run,
    selection: &mut Selection,
    loadout: &Loadout,
) -> bool {
    if net.in_room() {
        if net.leader() {
            if let Some(pick) = pick_of(net, selection, loadout) {
                net.send(&ClientMsg::SetPick(pick));
            }
            net.send(&ClientMsg::SetMap(selection.map as u16));
            net.send(&ClientMsg::Start);
            net.status = "Starting…".into();
        } else {
            net.status = "Waiting for the room's leader to start.".into();
        }
        return false;
    }
    hud::start_run(run, selection, session, loadout)
}

/// Read what the server sent: the room, the run's snapshots, the local hero, the events.
#[allow(clippy::too_many_arguments)]
pub(super) fn pump(
    real: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    mut next: ResMut<NextState<Phase>>,
    mut net: ResMut<Net>,
    mut session: ResMut<Session>,
    mut run: ResMut<Run>,
    mut selection: ResMut<Selection>,
    hero: Res<super::HeroLoadout>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let net = &mut *net;
        let Some(client) = &mut net.client else {
            return;
        };
        let now = real.elapsed_secs_f64();
        for msg in client.update() {
            match msg {
                ServerMsg::Lobby(l) => {
                    if net.lobby.as_ref().is_none_or(|old| old.code != l.code) {
                        net.status = format!("In room {}: share the code with your party.", l.code);
                    }
                    net.lobby = Some(l);
                }
                ServerMsg::NoRoom => {
                    net.lobby = None;
                    net.sent_pick = None;
                }
                ServerMsg::Started { map, you } => net.started = Some((map, you)),
                ServerMsg::Snapshot(s) => {
                    if session.online() {
                        session.push_snapshot(now, *s);
                    }
                }
                ServerMsg::Hero(h) => {
                    if session.online() && session.me == Some(h.id) {
                        let picked_through = session.picked && h.pending_levels < session.picked_from;
                        if !session.picked || picked_through {
                            session.picked = false;
                            session.offer = h.offer();
                        }
                        run.0 = h.to_run(Some(&run.0));
                    }
                }
                ServerMsg::Events(ev) => {
                    if session.online() {
                        session.events.extend(ev);
                    }
                }
                ServerMsg::Ended => {
                    if session.online() {
                        session.over = true;
                    }
                }
                ServerMsg::Error(e) => net.status = e,
            }
        }
        match client.status() {
            survivors_net::Status::Connected => {
                for msg in std::mem::take(&mut net.queued) {
                    client.send(&msg);
                }
            }
            survivors_net::Status::Connecting => {}
            survivors_net::Status::Disconnected(why) => {
                net.status = format!("Disconnected from {}: {why}", net.server);
                net.client = None;
                net.lobby = None;
                net.queued.clear();
                net.sent_pick = None;
                if session.online() {
                    session.over = true;
                }
            }
        }
    }

    // A run the server started: from the menu straight in; from the results, back to the menu
    // first so the last run's field is cleared.
    if let Some((map, you)) = net.started {
        match phase.get() {
            Phase::Menu => {
                net.started = None;
                selection.map = usize::from(map).min(data::MAPS.len() - 1);
                selection.stage = selection.map;
                selection.version += 1;
                session.start_online(you);
                let next_guid = run.next_guid;
                // The hero's own loadout: the server's hero updates keep it.
                run.0 = survivors_sim::Run {
                    next_guid,
                    loadout: hero.loadout.clone(),
                    ..Default::default()
                };
                net.status.clear();
                next.set(Phase::Playing);
            }
            Phase::GameOver | Phase::Paused => next.set(Phase::Menu),
            _ => {}
        }
    }
    let _ = (&real, &mut run);
}

/// Put this frame's messages on the wire.
pub(super) fn flush(mut net: ResMut<Net>) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(c) = &mut net.client {
        c.flush();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = &mut net;
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CoopAction {
    Host,
    Quick,
    Join,
    Leave,
    Server,
    Name,
}

#[derive(Component)]
pub(super) struct CoopBuilt;
#[derive(Component)]
pub(super) struct CoopStatus;
#[derive(Component)]
pub(super) struct CoopRoom;
#[derive(Component)]
pub(super) struct CoopEntry;

/// The room panel, top centre of the pick screen, once the glue art is in.
pub(super) fn build_panel(
    mut commands: Commands,
    roots: Query<Entity, (With<MenuRoot>, With<MenuBuilt>, Without<CoopBuilt>)>,
    art: Res<GlueArt>,
    font: Res<HudFont>,
) {
    let Ok(root) = roots.single() else { return };
    let f = &font.0;
    commands.entity(root).insert(CoopBuilt).with_children(|ui| {
        ui.spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|col| {
            col.spawn(hud::text_bundle(f, "Co-op", 16.0, hud::GOLD_TONE));
            col.spawn(Node {
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                for (action, label) in [
                    (CoopAction::Host, "Host Room"),
                    (CoopAction::Quick, "Quick Play"),
                    (CoopAction::Join, "Join Code"),
                    (CoopAction::Leave, "Leave Room"),
                ] {
                    glue_button(row, &art, f, action, label, 120.0, 32.0, GlueBtnKind::Small, 1.0);
                }
            });
            col.spawn(Node {
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                for (action, label) in [(CoopAction::Server, "Server"), (CoopAction::Name, "Name")] {
                    glue_button(row, &art, f, action, label, 90.0, 26.0, GlueBtnKind::Small, 1.0);
                }
            });
            col.spawn((CoopEntry, hud::text_bundle(f, "", 14.0, Color::WHITE)));
            col.spawn((CoopRoom, hud::centered_bundle(f, "", 14.0, Color::WHITE)));
            col.spawn((CoopStatus, hud::centered_bundle(f, "", 13.0, hud::DIM_GREY)));
        });
    });
}

/// The panel's buttons, and its text entry.
#[allow(clippy::too_many_arguments)]
pub(super) fn panel_input(
    clicks: Res<GlueClicks>,
    actions: Query<(Entity, &CoopAction)>,
    phase: Res<State<Phase>>,
    selection: Res<Selection>,
    loadout: Res<super::HeroLoadout>,
    mut keys_in: MessageReader<KeyboardInput>,
    mut net: ResMut<Net>,
) {
    if *phase.get() != Phase::Menu {
        keys_in.clear();
        return;
    }
    let pick = pick_of(&net, &selection, &loadout.loadout);
    for (e, action) in &actions {
        if !clicks.hit(e) {
            continue;
        }
        match action {
            CoopAction::Host | CoopAction::Quick => {
                let Some(pick) = pick.clone() else {
                    net.status = "Pick a hero first.".into();
                    continue;
                };
                let map = selection.map as u16;
                net.sent_pick = Some(pick.clone());
                net.request(if *action == CoopAction::Host {
                    ClientMsg::Create {
                        pick,
                        map,
                        public: false,
                    }
                } else {
                    ClientMsg::Quick { pick, map }
                });
            }
            CoopAction::Join => net.entry = Some((Field::Code, String::new())),
            CoopAction::Server => net.entry = Some((Field::Server, net.server.clone())),
            CoopAction::Name => net.entry = Some((Field::Name, net.name.clone())),
            CoopAction::Leave => {
                net.send(&ClientMsg::Leave);
                net.lobby = None;
                net.sent_pick = None;
                net.status = "Left the room.".into();
            }
        }
    }

    // Typing: letters, Backspace, Enter to take it, Escape to drop it.
    let mut submit = false;
    for k in keys_in.read() {
        let Some((field, text)) = net.entry.as_mut() else {
            continue;
        };
        if k.state != ButtonState::Pressed {
            continue;
        }
        let limit = match field {
            Field::Code => 4,
            Field::Server => 64,
            Field::Name => 16,
        };
        match &k.logical_key {
            Key::Character(c) => {
                for ch in c.chars() {
                    let ok = match field {
                        Field::Code => ch.is_ascii_alphabetic(),
                        Field::Server => ch.is_ascii_alphanumeric() || ".:-".contains(ch),
                        Field::Name => ch.is_alphanumeric(),
                    };
                    if ok && text.chars().count() < limit {
                        text.push(if *field == Field::Code {
                            ch.to_ascii_uppercase()
                        } else {
                            ch
                        });
                    }
                }
            }
            Key::Backspace => {
                text.pop();
            }
            Key::Enter => submit = true,
            Key::Escape => net.entry = None,
            _ => {}
        }
    }
    if submit {
        if let Some((field, text)) = net.entry.take() {
            match field {
                Field::Code if text.len() == 4 => match pick.clone() {
                    Some(pick) => {
                        net.sent_pick = Some(pick.clone());
                        net.request(ClientMsg::Join { code: text, pick });
                    }
                    None => net.status = "Pick a hero first.".into(),
                },
                Field::Code => net.status = "Room codes are four letters.".into(),
                Field::Server if !text.trim().is_empty() => {
                    if text != net.server {
                        net.disconnect();
                        net.server = text;
                        net.status = format!("Server set to {}.", net.server);
                        net.save();
                    }
                }
                Field::Name if !text.trim().is_empty() => {
                    net.name = text;
                    net.save();
                    net.sent_pick = None;
                }
                _ => {}
            }
        }
    }

    // A new hero picked while in a room: the room sees it.
    if net.in_room() && pick.is_some() && net.sent_pick != pick {
        if let Some(pick) = pick {
            net.send(&ClientMsg::SetPick(pick.clone()));
            net.sent_pick = Some(pick);
        }
    }
}

/// The panel's lines and which buttons apply.
#[allow(clippy::type_complexity)]
pub(super) fn refresh_panel(
    net: Res<Net>,
    mut texts: ParamSet<(
        Query<&mut Text, With<CoopStatus>>,
        Query<&mut Text, With<CoopRoom>>,
        Query<&mut Text, With<CoopEntry>>,
    )>,
    mut buttons: Query<(&CoopAction, &mut GlueDisabled)>,
) {
    let set = |t: &mut Text, s: String| {
        if t.0 != s {
            t.0 = s;
        }
    };
    let status = if net.status.is_empty() {
        format!("{} on {}", net.name, net.server)
    } else {
        net.status.clone()
    };
    if let Ok(mut t) = texts.p0().single_mut() {
        set(&mut t, status);
    }
    let room = match &net.lobby {
        None => String::new(),
        Some(l) => {
            let names: Vec<String> = l
                .members
                .iter()
                .map(|m| {
                    let class = data::CLASSES
                        .get(usize::from(m.pick.class))
                        .map_or("", |c| c.name);
                    let lead = if m.client == l.leader { " ★" } else { "" };
                    format!("{} ({class}){lead}", m.pick.name)
                })
                .collect();
            let map = data::MAPS
                .get(usize::from(l.map))
                .map_or("", |m| m.name);
            let state = if l.running {
                "run under way: Enter World joins it"
            } else if net.leader() {
                "you lead: Enter World starts for everyone"
            } else {
                "waiting for the leader"
            };
            format!(
                "Room {}  ·  {}/{}  ·  {map}  ·  {state}\n{}",
                l.code,
                l.members.len(),
                survivors_net::ROOM_SIZE,
                names.join("   ")
            )
        }
    };
    if let Ok(mut t) = texts.p1().single_mut() {
        set(&mut t, room);
    }
    let entry = match &net.entry {
        None => String::new(),
        Some((field, text)) => {
            let what = match field {
                Field::Code => "Room code",
                Field::Server => "Server address",
                Field::Name => "Your name",
            };
            format!("{what}: {text}_   (Enter to confirm, Esc to cancel)")
        }
    };
    if let Ok(mut t) = texts.p2().single_mut() {
        set(&mut t, entry);
    }
    let in_room = net.in_room();
    for (action, mut off) in &mut buttons {
        let want = match action {
            CoopAction::Leave => !in_room,
            CoopAction::Server | CoopAction::Name => in_room,
            _ => false,
        };
        if off.0 != want {
            off.0 = want;
        }
    }
}

#[derive(Component)]
pub(super) struct PartyFrame;
#[derive(Component)]
pub(super) struct PartyRow(usize);
#[derive(Component)]
pub(super) struct PartyName(usize);
#[derive(Component)]
pub(super) struct PartyFill(usize);
#[derive(Component)]
pub(super) struct PartyNote(usize);

const PARTY_BAR_W: f32 = 120.0;

/// Room for three teammates: the rows fill in from the snapshots.
pub(super) fn setup_party_frame(
    mut commands: Commands,
    roots: Query<Entity, With<HudRoot>>,
    frames: Query<(), With<PartyFrame>>,
    font: Option<Res<HudFont>>,
) {
    if !frames.is_empty() {
        return;
    }
    let (Ok(root), Some(font)) = (roots.single(), font) else {
        return;
    };
    let f = &font.0;
    commands.entity(root).with_children(|ui| {
        ui.spawn((
            PartyFrame,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(150.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            BorderColor::all(Color::srgba(0.55, 0.45, 0.25, 0.8)),
            Visibility::Hidden,
        ))
        .with_children(|col| {
            for i in 0..survivors_net::ROOM_SIZE - 1 {
                col.spawn((
                    PartyRow(i),
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        display: Display::None,
                        ..default()
                    },
                ))
                .with_children(|row| {
                    row.spawn((PartyName(i), hud::text_bundle(f, "", 13.0, Color::WHITE)));
                    row.spawn((
                        Node {
                            width: Val::Px(PARTY_BAR_W),
                            height: Val::Px(8.0),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                        BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            PartyFill(i),
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.1, 0.8, 0.1)),
                        ));
                    });
                    row.spawn((PartyNote(i), hud::text_bundle(f, "", 11.0, hud::DIM_GREY)));
                });
            }
        });
    });
}

/// The teammates' names, health and state, from the newest snapshot.
#[allow(clippy::type_complexity)]
pub(super) fn refresh_party_frame(
    session: Res<Session>,
    mut rows: Query<(&PartyRow, &mut Node), Without<PartyFill>>,
    mut names: Query<(&PartyName, &mut Text, &mut TextColor), Without<PartyNote>>,
    mut fills: Query<(&PartyFill, &mut Node, &mut BackgroundColor), Without<PartyRow>>,
    mut notes: Query<(&PartyNote, &mut Text), Without<PartyName>>,
    mut frame: Query<&mut Visibility, With<PartyFrame>>,
) {
    if let Ok(mut vis) = frame.single_mut() {
        let want = if session.online() && session.latest().is_some_and(|s| s.players.len() > 1) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
    let mates: Vec<&survivors_sim::PlayerSnap> = session
        .latest()
        .filter(|_| session.online())
        .map(|s| s.players.iter().filter(|p| Some(p.id) != session.me).collect())
        .unwrap_or_default();
    for (row, mut node) in &mut rows {
        let want = if row.0 < mates.len() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != want {
            node.display = want;
        }
    }
    for (n, mut t, mut color) in &mut names {
        if let Some(p) = mates.get(n.0) {
            let s = format!("{}  ·  {}", p.name, p.level);
            if t.0 != s {
                t.0 = s;
            }
            let class = data::CLASSES.iter().find(|c| c.id == p.class);
            let want = class.map_or(Color::WHITE, |c| hud::rgb(c.color));
            if color.0 != want {
                color.0 = want;
            }
        }
    }
    for (fill, mut node, mut bg) in &mut fills {
        if let Some(p) = mates.get(fill.0) {
            let frac = if p.down {
                p.revive
            } else {
                (p.hp / p.max_hp.max(1.0)).clamp(0.0, 1.0)
            };
            node.width = Val::Percent(frac * 100.0);
            bg.0 = if p.down {
                Color::srgb(0.9, 0.8, 0.2)
            } else if frac < 0.3 {
                Color::srgb(0.9, 0.15, 0.1)
            } else {
                Color::srgb(0.1, 0.8, 0.1)
            };
        }
    }
    let waiting = session.latest().is_some_and(|s| s.phase == 1) && session.offer.is_empty();
    for (n, mut t) in &mut notes {
        if let Some(p) = mates.get(n.0) {
            let s = if p.down {
                "Down! Stand beside them to revive".to_string()
            } else if waiting && p.picking {
                "Choosing a power…".to_string()
            } else {
                format!("{} kills", p.kills)
            };
            if t.0 != s {
                t.0 = s;
            }
        }
    }
}
