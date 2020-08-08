use futures::{
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    future::FutureExt,
    pin_mut, select,
};
use rudp::hand_shake::{client_connect, server_listen};
use rudp::{start_udp_loop, BypassResult};
use rudp_derive::PacketDesc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::sync::Notify;
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
        rotation: f32,
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
    pub static ref NETWORK: Mutex<Option<NetworkCommunication>> = Mutex::new(None);
    static ref GLOBAL_TIMER: Instant = Instant::now();
    static ref BG_TERMINATE: Notify = Notify::new();
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
async fn create_server_background_loop(port: u16) {
    // if there is a task waiting, the first one would release that, the second one is to allow our
    // next notified to continue.
    // there would only be 1 permit stored, so calling notify for two times without task waiting
    // would still be correct.
    BG_TERMINATE.notify();
    BG_TERMINATE.notify();
    BG_TERMINATE.notified().await;
    let f = async move {
        println!("Server linstening on 0.0.0.0:{}", port);
        let socket = server_listen(format!("0.0.0.0:{}", port).as_str(), MAGIC).await;
        println!("Connected!");
        let timeout = Duration::new(0, 20_000_000);
        let (send, recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
        let ping_send = send.clone();
        *NETWORK.lock().unwrap() = Some(NetworkCommunication::new(recv, send, Side::Server));
        loop {
            ping_send
                .unbounded_send(Packet::Ping(GLOBAL_TIMER.elapsed().as_millis()))
                .unwrap();
            delay_for(Duration::new(0, 100_000_000)).await;
        }
    }
    .fuse();
    let notify = BG_TERMINATE.notified().fuse();
    pin_mut!(f, notify);
    select! {
        _ = f => (),
        _ = notify => ()
    };
}

#[tokio::main]
async fn create_client_background_loop(addr: &str) {
    BG_TERMINATE.notify();
    BG_TERMINATE.notify();
    BG_TERMINATE.notified().await;
    let f = async move {
        println!("Client connecting...");
        let socket = client_connect("0.0.0.0:0", addr, MAGIC).await;
        println!("Client connected!");
        let timeout = Duration::new(0, 20_000_000);
        let (send, recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
        let ping_send = send.clone();
        *NETWORK.lock().unwrap() = Some(NetworkCommunication::new(recv, send, Side::Client));
        loop {
            ping_send
                .unbounded_send(Packet::Ping(GLOBAL_TIMER.elapsed().as_millis()))
                .unwrap();
            delay_for(Duration::new(0, 100_000_000)).await;
        }
    }
    .fuse();
    let notify = BG_TERMINATE.notified().fuse();
    pin_mut!(f, notify);
    select! {
        _ = f => (),
        _ = notify => ()
    };
}

pub fn init_server(port: u16) {
    thread::spawn(move || {
        println!("Start server...");
        create_server_background_loop(port);
        println!("End server");
    });
}

pub fn init_client(addr: String) {
    thread::spawn(move || {
        println!("Start client...");
        create_client_background_loop(&addr);
        println!("End client");
    });
}
