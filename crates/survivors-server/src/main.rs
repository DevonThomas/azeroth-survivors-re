//! `survivors-server`: the dedicated co-op server. Players connect over UDP (renet + netcode),
//! open or join rooms of up to four by a four-letter code (or Quick Play into any public room),
//! and each room's run is one [`Sim`] stepped here at [`TICK_HZ`]. The server holds the truth for
//! the horde, the spells, the party's experience and the cards; each client moves its own hero
//! over its own terrain and reports where it stands.
//!
//! It reads no game data: the battlegrounds' centres and rosters are the sim's own tables, and
//! enemies live on the ground plane (each client seats them on its terrain).

mod rooms;

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use renet::{RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
use survivors_net::{connection_config, name_from_user_data, PROTOCOL_ID, TICK_HZ};

#[derive(Parser)]
#[command(version, about = "Azeroth Survivors dedicated co-op server")]
struct Args {
    /// Address and port to listen on.
    #[arg(long, default_value = "0.0.0.0:7777", env = "AZS_BIND")]
    bind: SocketAddr,
    /// Most players connected at once (rooms are four each).
    #[arg(long, default_value_t = 256, env = "AZS_MAX_CLIENTS")]
    max_clients: usize,
    /// Print a status line this often (seconds), 0 for never.
    #[arg(long, default_value_t = 60, env = "AZS_STATUS_SECS")]
    status_secs: u64,
}

pub(crate) fn log(msg: impl AsRef<str>) {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (h, m, s) = ((t / 3600) % 24, (t / 60) % 60, t % 60);
    println!("[{h:02}:{m:02}:{s:02}Z] {}", msg.as_ref());
}

fn main() {
    let args = Args::parse();
    let socket = UdpSocket::bind(args.bind).unwrap_or_else(|e| {
        eprintln!("survivors-server: can't bind {}: {e}", args.bind);
        std::process::exit(1);
    });
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let config = ServerConfig {
        current_time: now,
        max_clients: args.max_clients,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![args.bind],
        // Unsecure: any client that speaks the protocol may connect. Good for a game among
        // friends; a public service would mint connect tokens from a login backend instead.
        authentication: ServerAuthentication::Unsecure,
    };
    let mut transport = NetcodeServerTransport::new(config, socket).expect("netcode transport");
    let mut server = RenetServer::new(connection_config());
    let mut rooms = rooms::Rooms::default();
    log(format!(
        "listening on udp {} (protocol {PROTOCOL_ID:#x}, {TICK_HZ} Hz)",
        args.bind
    ));

    let tick = Duration::from_secs(1) / TICK_HZ;
    let mut last = Instant::now();
    let mut next = Instant::now();
    let mut status_at = Instant::now() + Duration::from_secs(args.status_secs.max(1));
    loop {
        let now = Instant::now();
        let dt = now - last;
        last = now;
        server.update(dt);
        if let Err(e) = transport.update(dt, &mut server) {
            log(format!("transport: {e}"));
        }

        while let Some(event) = server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    let name = transport
                        .user_data(client_id)
                        .map(|d| name_from_user_data(&d))
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or_else(|| "Adventurer".into());
                    let addr = transport
                        .client_addr(client_id)
                        .map_or_else(String::new, |a| a.to_string());
                    log(format!("+ {client_id:016x} {name} {addr}"));
                    rooms.connect(client_id, name);
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    log(format!("- {client_id:016x} ({reason})"));
                    rooms.disconnect(&mut server, client_id);
                }
            }
        }
        for client_id in server.clients_id() {
            for channel in [survivors_net::RELIABLE, survivors_net::UNRELIABLE] {
                while let Some(bytes) = server.receive_message(client_id, channel) {
                    match survivors_net::decode(&bytes) {
                        Some(msg) => rooms.handle(&mut server, client_id, msg),
                        None => log(format!("? {client_id:016x} sent an unreadable message")),
                    }
                }
            }
        }

        if now >= next {
            next += tick;
            // Fell far behind (a stall, a suspended VM): don't try to catch up in a burst.
            if now > next + tick * 10 {
                next = now + tick;
            }
            rooms.tick(&mut server, 1.0 / TICK_HZ as f32);
        }
        transport.send_packets(&mut server);

        if args.status_secs > 0 && now >= status_at {
            status_at = now + Duration::from_secs(args.status_secs);
            log(rooms.status(server.connected_clients()));
        }
        let wait = next.saturating_duration_since(Instant::now());
        std::thread::sleep(wait.min(Duration::from_millis(5)));
    }
}
