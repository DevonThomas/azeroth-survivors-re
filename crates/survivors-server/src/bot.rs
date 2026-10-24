//! `survivors-bot`: headless players for testing a server. Each bot connects, gets into a room,
//! walks circles around the battleground, takes the first card it is dealt and reports what it
//! received. `survivors-bot --server 127.0.0.1:7777 --bots 3 --secs 60` fills a room and starts
//! it; `--join ABCD` sends them into a room someone else opened (a real client, say).

use std::time::{Duration, Instant};

use clap::Parser;
use survivors_net::{Client, ClientMsg, InputMsg, Pick, ServerMsg, Status};
use survivors_sim::{data, Event, Vec2, Vec3};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:7777")]
    server: String,
    #[arg(long, default_value_t = 2)]
    bots: usize,
    /// Join this room instead of opening one.
    #[arg(long)]
    join: Option<String>,
    #[arg(long, default_value_t = 0)]
    map: u16,
    #[arg(long, default_value_t = 60)]
    secs: u64,
}

struct Bot {
    name: String,
    client: Client,
    you: Option<u32>,
    seq: u32,
    pos: Vec2,
    snapshots: u32,
    events: u32,
    max_enemies: usize,
    level: u32,
    ended: bool,
    code: Option<String>,
    members: usize,
}

fn main() {
    let args = Args::parse();
    let mut bots: Vec<Bot> = (0..args.bots)
        .map(|i| {
            let name = format!("Bot{}", i + 1);
            Bot {
                client: Client::connect(&args.server, &name).expect("connect"),
                name,
                you: None,
                seq: 0,
                pos: Vec2::ZERO,
                snapshots: 0,
                events: 0,
                max_enemies: 0,
                level: 1,
                ended: false,
                code: None,
                members: 0,
            }
        })
        .collect();
    let started = Instant::now();
    let mut asked = vec![false; bots.len()];
    let mut start_sent = false;
    let center = {
        let c = data::MAPS[usize::from(args.map)].center;
        Vec2::new(-c[1], -c[0])
    };
    while started.elapsed() < Duration::from_secs(args.secs) {
        let t = started.elapsed().as_secs_f32();
        let leader_code = bots[0].code.clone();
        for (i, bot) in bots.iter_mut().enumerate() {
            for msg in bot.client.update() {
                match msg {
                    ServerMsg::Lobby(l) => {
                        bot.code = Some(l.code.clone());
                        bot.members = l.members.len();
                        if i == 0 && bot.members == 1 {
                            println!("{}: room {}", bot.name, l.code);
                        }
                    }
                    ServerMsg::Started { you, .. } => {
                        bot.you = Some(you);
                        bot.pos = center + Vec2::new(i as f32 * 2.0, 0.0);
                        println!("{}: in the run as hero {you}", bot.name);
                    }
                    ServerMsg::Snapshot(s) => {
                        bot.snapshots += 1;
                        bot.max_enemies = bot.max_enemies.max(s.enemies.len());
                        bot.level = s.level;
                    }
                    ServerMsg::Hero(h) => {
                        if !h.offer.is_empty() {
                            bot.client.send(&ClientMsg::Choose { index: 0 });
                        }
                    }
                    ServerMsg::Events(ev) => {
                        bot.events += ev.len() as u32;
                        for e in ev {
                            if let Event::Teleport { player, to, seq, .. } = e {
                                if Some(player) == bot.you {
                                    bot.pos = to;
                                    bot.seq = seq;
                                }
                            }
                        }
                    }
                    ServerMsg::Ended => bot.ended = true,
                    ServerMsg::Error(e) => println!("{}: server says {e}", bot.name),
                    ServerMsg::NoRoom => bot.code = None,
                }
            }
            if bot.client.status() == Status::Connected && !asked[i] {
                let pick = Pick {
                    name: bot.name.clone(),
                    class: (i * 3 % data::CLASSES.len()) as u8,
                    race: 1,
                    sex: 0,
                    display: 0,
                };
                let msg = match (&args.join, i, &leader_code) {
                    (Some(code), _, _) => Some(ClientMsg::Join {
                        code: code.clone(),
                        pick,
                    }),
                    (None, 0, _) => Some(ClientMsg::Create {
                        pick,
                        map: args.map,
                        public: false,
                    }),
                    (None, _, Some(code)) => Some(ClientMsg::Join {
                        code: code.clone(),
                        pick,
                    }),
                    _ => None,
                };
                if let Some(msg) = msg {
                    bot.client.send(&msg);
                    asked[i] = true;
                }
            }
            if bot.you.is_some() && !bot.ended {
                // Circles around the battleground, each bot on its own.
                let a = t * 0.4 + i as f32;
                let want = center + Vec2::new(a.cos(), a.sin()) * 12.0;
                let step = (want - bot.pos).clamp_length_max(7.0 / 60.0);
                bot.pos += step;
                bot.client.send(&ClientMsg::Input(InputMsg {
                    pos: Vec3::new(bot.pos.x, 0.0, bot.pos.y),
                    yaw: 0.0,
                    flags: 1,
                    speed: 7.0,
                    vy: 0.0,
                    seq: bot.seq,
                }));
            }
            bot.client.flush();
        }
        if args.join.is_none() && !start_sent && bots[0].members == bots.len() {
            bots[0].client.send(&ClientMsg::Start);
            start_sent = true;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    for bot in &bots {
        println!(
            "{}: {:?}, rtt {:.0} ms, {} snapshots, {} events, peak {} enemies, party level {}{}",
            bot.name,
            bot.client.status(),
            bot.client.rtt_ms(),
            bot.snapshots,
            bot.events,
            bot.max_enemies,
            bot.level,
            if bot.ended { ", run ended" } else { "" }
        );
    }
}
