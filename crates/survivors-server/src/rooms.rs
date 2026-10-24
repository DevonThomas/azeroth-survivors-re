//! Rooms: a lobby of up to four, and the run they play together.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use renet::RenetServer;
use survivors_net::{
    encode, normalize_code, ClientMsg, HeroState, Lobby, Member, Pick, ServerMsg, CODE_ALPHABET,
    RELIABLE, ROOM_SIZE, SNAPSHOT_EVERY, UNRELIABLE,
};
use survivors_sim::{data, Event, Id, Rng, Sim, SimPhase};

use crate::log;

/// Seconds a finished run's room waits before it is back in its lobby.
const RESULTS_SECS: f32 = 2.0;

struct Room {
    code: String,
    leader: u64,
    map: u16,
    public: bool,
    members: Vec<Member>,
    sim: Option<Sim>,
    tick: u32,
    /// Seconds since the run ended, while the last snapshot stands.
    ended_t: Option<f32>,
}

impl Room {
    fn lobby(&self) -> Lobby {
        Lobby {
            code: self.code.clone(),
            leader: self.leader,
            map: self.map,
            public: self.public,
            members: self.members.clone(),
            running: self.sim.is_some(),
        }
    }

    fn has_space(&self) -> bool {
        self.members.len() < ROOM_SIZE
    }

    fn broadcast(&self, server: &mut RenetServer, channel: u8, msg: &ServerMsg) {
        broadcast(server, &self.members, channel, msg);
    }
}

fn broadcast(server: &mut RenetServer, members: &[Member], channel: u8, msg: &ServerMsg) {
    let bytes = encode(msg);
    for m in members {
        server.send_message(m.client, channel, bytes.clone());
    }
}

#[derive(Default)]
pub(crate) struct Rooms {
    names: HashMap<u64, String>,
    /// Which room each client is in.
    of: HashMap<u64, String>,
    rooms: HashMap<String, Room>,
    rng: Option<Rng>,
}

fn clock_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

fn send(server: &mut RenetServer, client: u64, msg: &ServerMsg) {
    server.send_message(client, RELIABLE, encode(msg));
}

impl Rooms {
    fn rng(&mut self) -> &mut Rng {
        self.rng.get_or_insert_with(|| Rng::from_clock(clock_nanos()))
    }

    fn new_code(&mut self) -> String {
        loop {
            let code: String = (0..4)
                .map(|_| {
                    let i = self.rng().index(CODE_ALPHABET.len());
                    CODE_ALPHABET[i] as char
                })
                .collect();
            if !self.rooms.contains_key(&code) {
                return code;
            }
        }
    }

    pub fn connect(&mut self, client: u64, name: String) {
        self.names.insert(client, name);
    }

    pub fn disconnect(&mut self, server: &mut RenetServer, client: u64) {
        self.leave(server, client);
        self.names.remove(&client);
    }

    fn pick_named(&self, client: u64, mut pick: Pick) -> Pick {
        if pick.name.trim().is_empty() {
            pick.name = self.names.get(&client).cloned().unwrap_or_default();
        }
        pick.name = pick.name.chars().take(16).collect();
        pick.class = pick.class.min(data::CLASSES.len() as u8 - 1);
        pick
    }

    pub fn handle(&mut self, server: &mut RenetServer, client: u64, msg: ClientMsg) {
        match msg {
            ClientMsg::Create { pick, map, public } => {
                self.leave(server, client);
                let code = self.new_code();
                let pick = self.pick_named(client, pick);
                log(format!("room {code} opened by {}", pick.name));
                self.rooms.insert(
                    code.clone(),
                    Room {
                        code: code.clone(),
                        leader: client,
                        map: map.min(data::MAPS.len() as u16 - 1),
                        public,
                        members: Vec::new(),
                        sim: None,
                        tick: 0,
                        ended_t: None,
                    },
                );
                self.enter(server, client, &code, pick);
            }
            ClientMsg::Join { code, pick } => {
                let code = normalize_code(&code);
                let pick = self.pick_named(client, pick);
                match self.rooms.get(&code) {
                    None => send(server, client, &ServerMsg::Error(format!("No room {code}."))),
                    Some(r) if !r.has_space() => {
                        send(server, client, &ServerMsg::Error(format!("Room {code} is full.")));
                    }
                    Some(_) => {
                        if self.of.get(&client) != Some(&code) {
                            self.leave(server, client);
                            self.enter(server, client, &code, pick);
                        }
                    }
                }
            }
            ClientMsg::Quick { pick, map } => {
                let pick = self.pick_named(client, pick);
                // A public room with space, liveliest first; a fresh public room otherwise.
                let found = self
                    .rooms
                    .values()
                    .filter(|r| r.public && r.has_space() && r.ended_t.is_none())
                    .max_by_key(|r| (r.members.len(), r.sim.is_none()))
                    .map(|r| r.code.clone());
                match found {
                    Some(code) => {
                        if self.of.get(&client) != Some(&code) {
                            self.leave(server, client);
                            self.enter(server, client, &code, pick);
                        }
                    }
                    None => self.handle(
                        server,
                        client,
                        ClientMsg::Create {
                            pick,
                            map,
                            public: true,
                        },
                    ),
                }
            }
            ClientMsg::SetPick(pick) => {
                let pick = self.pick_named(client, pick);
                if let Some(room) = self.room_of_mut(client) {
                    if let Some(m) = room.members.iter_mut().find(|m| m.client == client) {
                        if m.hero.is_none() && m.pick != pick {
                            m.pick = pick;
                            let lobby = ServerMsg::Lobby(room.lobby());
                            room.broadcast(server, RELIABLE, &lobby);
                        }
                    }
                }
            }
            ClientMsg::SetMap(map) => {
                if let Some(room) = self.room_of_mut(client) {
                    if room.leader == client && room.sim.is_none() {
                        room.map = map.min(data::MAPS.len() as u16 - 1);
                        let lobby = ServerMsg::Lobby(room.lobby());
                        room.broadcast(server, RELIABLE, &lobby);
                    }
                }
            }
            ClientMsg::Start => {
                let seed = self.rng().next_u64();
                if let Some(room) = self.room_of_mut(client) {
                    if room.leader == client && room.sim.is_none() {
                        start(server, room, seed);
                    }
                }
            }
            ClientMsg::Leave => self.leave(server, client),
            ClientMsg::Input(input) => {
                if let Some((room, hero)) = self.hero_of(client) {
                    if let Some(sim) = &mut room.sim {
                        sim.set_input(hero, input.into());
                    }
                }
            }
            ClientMsg::Mobility { aim } => {
                if let Some((room, hero)) = self.hero_of(client) {
                    if let Some(sim) = &mut room.sim {
                        sim.cast_mobility(hero, aim);
                    }
                }
            }
            ClientMsg::Choose { index } => {
                if let Some((room, hero)) = self.hero_of(client) {
                    if let Some(sim) = &mut room.sim {
                        sim.choose(hero, usize::from(index));
                    }
                }
            }
        }
    }

    fn room_of_mut(&mut self, client: u64) -> Option<&mut Room> {
        let code = self.of.get(&client)?;
        self.rooms.get_mut(code)
    }

    fn hero_of(&mut self, client: u64) -> Option<(&mut Room, Id)> {
        let room = self.room_of_mut(client)?;
        let hero = room.members.iter().find(|m| m.client == client)?.hero?;
        Some((room, hero))
    }

    fn enter(&mut self, server: &mut RenetServer, client: u64, code: &str, pick: Pick) {
        let Some(room) = self.rooms.get_mut(code) else {
            return;
        };
        let mut member = Member {
            client,
            pick,
            hero: None,
        };
        // A run under way: straight in, catching up on the party's levels card by card.
        if let Some(sim) = &mut room.sim {
            let p = &member.pick;
            member.hero = sim.add_player(&p.name, usize::from(p.class), p.race, p.sex, p.display);
            if let Some(you) = member.hero {
                send(
                    server,
                    client,
                    &ServerMsg::Started {
                        map: room.map,
                        you,
                    },
                );
            }
        }
        log(format!(
            "{} joined room {code} ({}/{ROOM_SIZE})",
            member.pick.name,
            room.members.len() + 1
        ));
        room.members.push(member);
        self.of.insert(client, code.to_string());
        let lobby = ServerMsg::Lobby(room.lobby());
        room.broadcast(server, RELIABLE, &lobby);
    }

    fn leave(&mut self, server: &mut RenetServer, client: u64) {
        let Some(code) = self.of.remove(&client) else {
            return;
        };
        if server.is_connected(client) {
            send(server, client, &ServerMsg::NoRoom);
        }
        let Some(room) = self.rooms.get_mut(&code) else {
            return;
        };
        if let Some(i) = room.members.iter().position(|m| m.client == client) {
            let m = room.members.remove(i);
            if let (Some(sim), Some(hero)) = (&mut room.sim, m.hero) {
                sim.remove_player(hero);
            }
        }
        if room.members.is_empty() {
            log(format!("room {code} closed"));
            self.rooms.remove(&code);
            return;
        }
        if room.leader == client {
            room.leader = room.members[0].client;
        }
        let lobby = ServerMsg::Lobby(room.lobby());
        room.broadcast(server, RELIABLE, &lobby);
    }

    /// Step every running room, and stream what happened.
    pub fn tick(&mut self, server: &mut RenetServer, dt: f32) {
        let mut to_lobby = Vec::new();
        for room in self.rooms.values_mut() {
            let Some(sim) = &mut room.sim else { continue };
            if let Some(t) = &mut room.ended_t {
                *t += dt;
                if *t >= RESULTS_SECS {
                    to_lobby.push(room.code.clone());
                }
                continue;
            }
            sim.step(dt);
            room.tick = room.tick.wrapping_add(1);
            let events = sim.drain_events();
            let (sure, look): (Vec<Event>, Vec<Event>) =
                events.into_iter().partition(Event::reliable);
            let over = sim.phase() == SimPhase::Over;
            let snapshot_due = over || room.tick % SNAPSHOT_EVERY == 0;
            let snapshot = snapshot_due.then(|| ServerMsg::Snapshot(Box::new(sim.snapshot())));
            let heroes: Vec<(u64, Option<HeroState>)> = if snapshot_due {
                room.members
                    .iter()
                    .map(|m| (m.client, m.hero.and_then(|h| HeroState::of(sim, h))))
                    .collect()
            } else {
                Vec::new()
            };
            if !sure.is_empty() {
                broadcast(server, &room.members, RELIABLE, &ServerMsg::Events(sure));
            }
            if !look.is_empty() {
                // Fine-grained visuals: chunk them so no one message grows past a few packets.
                for chunk in look.chunks(96) {
                    broadcast(server, &room.members, UNRELIABLE, &ServerMsg::Events(chunk.to_vec()));
                }
            }
            if let Some(snapshot) = snapshot {
                // The final snapshot carries the result: it must land.
                let channel = if over { RELIABLE } else { UNRELIABLE };
                broadcast(server, &room.members, channel, &snapshot);
            }
            for (client, hero) in heroes {
                if let Some(hero) = hero {
                    let channel = if over { RELIABLE } else { UNRELIABLE };
                    server.send_message(client, channel, encode(&ServerMsg::Hero(Box::new(hero))));
                }
            }
            if over {
                room.ended_t = Some(0.0);
                broadcast(server, &room.members, RELIABLE, &ServerMsg::Ended);
                log(format!(
                    "room {} run over at {:.0}s",
                    room.code,
                    sim.elapsed()
                ));
            }
        }
        for code in to_lobby {
            if let Some(room) = self.rooms.get_mut(&code) {
                room.sim = None;
                room.ended_t = None;
                for m in &mut room.members {
                    m.hero = None;
                }
                let lobby = ServerMsg::Lobby(room.lobby());
                room.broadcast(server, RELIABLE, &lobby);
            }
        }
    }

    pub fn status(&self, clients: usize) -> String {
        let running = self.rooms.values().filter(|r| r.sim.is_some()).count();
        format!(
            "{clients} connected, {} rooms ({running} running)",
            self.rooms.len()
        )
    }
}

fn start(server: &mut RenetServer, room: &mut Room, seed: u64) {
    let mut sim = Sim::new(usize::from(room.map), seed);
    for m in &mut room.members {
        let p = &m.pick;
        m.hero = sim.add_player(&p.name, usize::from(p.class), p.race, p.sex, p.display);
    }
    log(format!(
        "room {} started on {} with {}",
        room.code,
        data::MAPS[usize::from(room.map)].name,
        room.members
            .iter()
            .map(|m| m.pick.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    for m in &room.members {
        if let Some(you) = m.hero {
            send(
                server,
                m.client,
                &ServerMsg::Started {
                    map: room.map,
                    you,
                },
            );
        }
    }
    room.sim = Some(sim);
    room.tick = 0;
    room.ended_t = None;
    let lobby = ServerMsg::Lobby(room.lobby());
    room.broadcast(server, RELIABLE, &lobby);
}
