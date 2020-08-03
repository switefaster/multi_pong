use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use rand::Rng;
use rudp::hand_shake::{client_connect, server_listen};
use rudp::{start_udp_loop, BypassResult};
use rudp_derive::PacketDesc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{delay_for, Instant};

pub enum Side {
    Server,
    Client,
}

#[derive(Default)]
pub struct NetworkCommunication {
    pub(crate) receiver: Option<UnboundedReceiver<Packet>>,
    pub(crate) sender: Option<Arc<UnboundedSender<Packet>>>,
    side: Option<Side>,
}

impl NetworkCommunication {
    pub fn new(
        receiver: UnboundedReceiver<Packet>,
        sender: Arc<UnboundedSender<Packet>>,
        side: Side,
    ) -> Self {
        Self {
            receiver: Some(receiver),
            sender: Some(sender),
            side: Some(side),
        }
    }

    pub fn is_client(&self) -> bool {
        if let Some(Side::Client) = self.side {
            true
        } else {
            false
        }
    }

    pub fn is_server(&self) -> bool {
        !self.is_client() && self.side.is_some()
    }
}

#[derive(PacketDesc, serde::Serialize, serde::Deserialize)]
pub enum Packet {
    #[packet(reliable)]
    Handshake {
        player_name: String,
    },
    #[packet(ordered)]
    PaddleDisplace {
        position: f32,
    },
    #[packet(ordered)]
    BallPosVel {
        position: [f32; 2],
        velocity: [f32; 2],
    },
    Ping(u128),
    Pong(u128),
}

const MAGIC: &[u8] = b"MULTI_PONG";
lazy_static::lazy_static! {
    pub static ref GLOBAL_PING: Mutex<u128> = Mutex::new(0);
}
lazy_static::lazy_static! {
    static ref GLOBAL_TIMER: Instant = Instant::now();
}

fn bypass(p: Packet) -> BypassResult<Packet> {
    match p {
        Packet::Ping(i) => BypassResult::ToSender(Packet::Pong(i)),
        Packet::Pong(i) => {
            let now = GLOBAL_TIMER.elapsed();
            *GLOBAL_PING.lock().unwrap() = now.as_millis() - i;
            BypassResult::Discard
        }
        _ => BypassResult::ToUser(p),
    }
}

#[tokio::main]
pub async fn create_server_background_loop(port: u16) -> NetworkCommunication {
    let socket = server_listen(format!("0.0.0.0:{}", port).as_str(), MAGIC).await;
    let timeout = Duration::new(0, 20_000_000);
    let (send, recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
    let ping_send = send.clone();
    tokio::spawn(async move {
        loop {
            ping_send
                .unbounded_send(Packet::Ping(GLOBAL_TIMER.elapsed().as_millis()))
                .unwrap();
            delay_for(Duration::new(0, 100_000_000)).await;
        }
    });
    NetworkCommunication::new(recv, send, Side::Server)
}

#[tokio::main]
pub async fn create_client_background_loop(addr: &str) -> NetworkCommunication {
    let port: u16 = rand::thread_rng().gen();
    let socket = client_connect(format!("0.0.0.0:{}", port).as_str(), addr, MAGIC).await;
    let timeout = Duration::new(0, 20_000_000);
    let (send, recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
    let ping_send = send.clone();
    tokio::spawn(async move {
        loop {
            ping_send
                .unbounded_send(Packet::Ping(GLOBAL_TIMER.elapsed().as_millis()))
                .unwrap();
            delay_for(Duration::new(0, 100_000_000)).await;
        }
    });
    NetworkCommunication::new(recv, send, Side::Client)
}
