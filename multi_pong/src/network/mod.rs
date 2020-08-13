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
    Handshake { player_name: String },
    #[packet(ordered)]
    PaddleDisplace { position: f32, rotation: f32 },
    #[packet(ordered)]
    BallPosVel {
        position: [f32; 2],
        velocity: [f32; 2],
    },
    #[packet(ordered)]
    Ping {
        client_time: i128,
        expected_arrival: i128,
    },
    #[packet(ordered)]
    Pong {
        client_time: i128,
        remote_time: i128,
    },
}

pub struct State {
    pub start_time: Instant,
    // timer offset from remote to local, in microsecond
    pub time_offset: [i128; 16],
    // decided offset
    pub qin_ding_offset: i128,
    // one way latency, in microsecond
    pub latency: [i128; 16],
    // current index
    pub index: usize,
    // actual offset from client in microsecond
    pub actual_offset: i128,
}

const MAGIC: &[u8] = b"MULTI_PONG";
lazy_static::lazy_static! {
    pub static ref STATE: Mutex<State> = Mutex::new(State {
        start_time: Instant::now(),
        time_offset: [0; 16],
        latency: [0; 16],
        qin_ding_offset: 0,
        index: 0,
        actual_offset: 0
    });
    pub static ref NETWORK: Mutex<Option<NetworkCommunication>> = Mutex::new(None);
    static ref BG_TERMINATE: Notify = Notify::new();
}

fn bypass(p: Packet) -> BypassResult<Packet> {
    match p {
        Packet::Ping {
            client_time,
            expected_arrival,
        } => {
            let mut lock = STATE.lock();
            let state = lock.as_mut().unwrap();
            let remote_time = state.start_time.elapsed().as_micros() as i128;
            let actual_offset = expected_arrival - remote_time;
            state.actual_offset = actual_offset;
            BypassResult::ToSender(Packet::Pong {
                client_time,
                remote_time,
            })
        }
        Packet::Pong {
            client_time,
            remote_time,
        } => {
            let mut lock = STATE.lock();
            let state = lock.as_mut().unwrap();
            let now = state.start_time.elapsed().as_micros() as i128;
            let raw_latency = (now - client_time) / 2;
            let raw_offset = (remote_time - client_time) - raw_latency;

            let index = state.index;
            if index < state.latency.len() {
                state.latency[index] = raw_latency;
                state.time_offset[index] = raw_offset;
                state.index += 1;
                if index == state.latency.len() - 1 {
                    let &offset = state
                        .latency
                        .iter()
                        .zip(state.time_offset.iter())
                        .min_by_key(|v| v.0)
                        .unwrap()
                        .1;
                    state.qin_ding_offset = offset;
                }
            } else {
                let index = state.latency.len() - 1;
                state.latency[index] = raw_latency;
                state.time_offset[index] = raw_offset;
            }
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
        let interval = Duration::new(0, 100_000_000);
        loop {
            delay_for(interval).await;
            let (start, offset, latency, _) = {
                let lock = STATE.lock();
                let state = lock.as_ref().unwrap();
                let index = if state.index < state.latency.len() {
                    state.index
                } else {
                    state.latency.len() - 1
                };
                (
                    state.start_time,
                    state.qin_ding_offset,
                    state.latency[index],
                    state.actual_offset,
                )
            };
            let client_time = start.elapsed().as_micros() as i128;
            let expected_arrival = client_time + offset + latency;
            let packet = Packet::Ping {
                client_time,
                expected_arrival,
            };
            if ping_send.unbounded_send(packet).is_err() {
                return;
            }
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
        let interval = Duration::new(0, 100_000_000);
        loop {
            delay_for(interval).await;
            let (start, offset, latency, _) = {
                let lock = STATE.lock();
                let state = lock.as_ref().unwrap();
                let index = if state.index < state.latency.len() {
                    state.index
                } else {
                    state.latency.len() - 1
                };
                (
                    state.start_time,
                    state.qin_ding_offset,
                    state.latency[index],
                    state.actual_offset,
                )
            };
            let client_time = start.elapsed().as_micros() as i128;
            let expected_arrival = client_time + offset + latency;
            let packet = Packet::Ping {
                client_time,
                expected_arrival,
            };
            if ping_send.unbounded_send(packet).is_err() {
                return;
            }
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
