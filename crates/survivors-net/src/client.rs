//! The client's end: a renet connection over netcode to the dedicated server.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use crate::{connection_config, decode, encode, user_data, ClientMsg, ServerMsg, PROTOCOL_ID};
use crate::{RELIABLE, UNRELIABLE};

#[derive(Debug)]
pub enum ClientError {
    /// The address did not resolve.
    Resolve(String),
    Io(std::io::Error),
    Netcode(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolve(a) => write!(f, "can't find server {a}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Netcode(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Connected,
    Disconnected(String),
}

pub struct Client {
    renet: RenetClient,
    transport: NetcodeClientTransport,
    last: Instant,
    pub addr: SocketAddr,
    client_id: u64,
    failure: Option<String>,
}

impl Client {
    /// Start connecting to `addr` (`host:port`; the port defaults to [`crate::DEFAULT_PORT`]).
    pub fn connect(addr: &str, name: &str) -> Result<Self, ClientError> {
        let with_port = if addr.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) {
            addr.to_string()
        } else {
            format!("{addr}:{}", crate::DEFAULT_PORT)
        };
        let server = with_port
            .to_socket_addrs()
            .map_err(|_| ClientError::Resolve(addr.into()))?
            .find(SocketAddr::is_ipv4)
            .ok_or_else(|| ClientError::Resolve(addr.into()))?;
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(ClientError::Io)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let client_id = (now.as_nanos() as u64) ^ (u64::from(std::process::id()) << 40);
        let auth = ClientAuthentication::Unsecure {
            server_addr: server,
            client_id,
            user_data: Some(user_data(name)),
            protocol_id: PROTOCOL_ID,
        };
        let transport = NetcodeClientTransport::new(now, auth, socket)
            .map_err(|e| ClientError::Netcode(e.to_string()))?;
        Ok(Self {
            renet: RenetClient::new(connection_config()),
            transport,
            last: Instant::now(),
            addr: server,
            client_id,
            failure: None,
        })
    }

    pub fn id(&self) -> u64 {
        self.client_id
    }

    /// Pump the socket and return everything the server sent since the last call.
    pub fn update(&mut self) -> Vec<ServerMsg> {
        let now = Instant::now();
        let dt = now - self.last;
        self.last = now;
        self.renet.update(dt);
        if let Err(e) = self.transport.update(dt, &mut self.renet) {
            self.failure.get_or_insert_with(|| e.to_string());
        }
        let mut out = Vec::new();
        for channel in [RELIABLE, UNRELIABLE] {
            while let Some(bytes) = self.renet.receive_message(channel) {
                if let Some(msg) = decode::<ServerMsg>(&bytes) {
                    out.push(msg);
                }
            }
        }
        out
    }

    pub fn send(&mut self, msg: &ClientMsg) {
        if !self.renet.is_connected() {
            return;
        }
        let channel = match msg {
            ClientMsg::Input(_) => UNRELIABLE,
            _ => RELIABLE,
        };
        self.renet.send_message(channel, encode(msg));
    }

    /// Put everything sent this frame on the wire.
    pub fn flush(&mut self) {
        if let Err(e) = self.transport.send_packets(&mut self.renet) {
            self.failure.get_or_insert_with(|| e.to_string());
        }
    }

    pub fn status(&self) -> Status {
        if let Some(reason) = self
            .renet
            .disconnect_reason()
            .map(|r| r.to_string())
            .or_else(|| self.transport.disconnect_reason().map(|r| r.to_string()))
        {
            return Status::Disconnected(reason);
        }
        if let Some(e) = &self.failure {
            return Status::Disconnected(e.clone());
        }
        if self.renet.is_connected() {
            Status::Connected
        } else {
            Status::Connecting
        }
    }

    /// Round trip in milliseconds.
    pub fn rtt_ms(&self) -> f64 {
        self.renet.rtt() * 1000.0
    }

    pub fn packet_loss(&self) -> f64 {
        self.renet.packet_loss()
    }

    /// Say goodbye so the server frees the slot at once rather than at the timeout.
    pub fn disconnect(&mut self) {
        self.transport.disconnect();
    }

    /// How long since the server was last heard from.
    pub fn silence(&self) -> Duration {
        self.transport.time_since_last_received_packet()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.transport.disconnect();
    }
}
