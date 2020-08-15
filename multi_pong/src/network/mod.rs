use futures::{
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    future::FutureExt,
    pin_mut, select,
};
use rudp::hand_shake::{client_connect, server_listen};
use rudp::{start_udp_loop, BypassResult};
use rudp_derive::PacketDesc;
use std::sync::{
    atomic::{AtomicI64, Ordering::Relaxed},
    Arc, Mutex,
};
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

struct State {
    pub start_time: Instant,
    // timer offset from remote to local, in microsecond
    pub time_offset: [i128; 16],
    // decided offset
    pub qin_ding_offset: i128,
    // one way latency, in microsecond
    pub latency: [i128; 16],
    // current index
    pub index: usize,
    // dead count
    pub dead: u32,
}

const MAGIC: &[u8] = b"MULTI_PONG";
pub static PING_LATENCY: AtomicI64 = AtomicI64::new(0);
lazy_static::lazy_static! {
    static ref STATE: Mutex<State> = Mutex::new(State {
        start_time: Instant::now(),
        time_offset: [0; 16],
        latency: [0; 16],
        qin_ding_offset: 0,
        index: 0,
        dead: 0,
    });
    pub static ref NETWORK: Mutex<Option<(NetworkCommunication, Instant)>> = Mutex::new(None);
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
            log::debug!("Remote time offset: {:>6}μs", actual_offset);
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
            state.dead = 0;
            let now = state.start_time.elapsed().as_micros() as i128;
            let raw_latency = (now - client_time) / 2;

            let index = state.index;
            if index < state.latency.len() {
                state.latency[index] = raw_latency;
                state.time_offset[index] = (remote_time - client_time) - raw_latency;
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
                PING_LATENCY.store(raw_latency as i64, Relaxed);
            }
            BypassResult::Discard
        }
        _ => BypassResult::ToUser(p),
    }
}

fn ping_packet() -> Packet {
    let (start, offset) = {
        let mut lock = STATE.lock();
        let state = lock.as_mut().unwrap();
        state.dead += 1;
        // TODO: if dead exceeded certain threshold, we should tell the game to stop.
        if state.dead > 50 {
            log::warn!("5s without response!");
        }
        (state.start_time, state.qin_ding_offset)
    };
    let latency = PING_LATENCY.load(Relaxed) as i128;
    let client_time = start.elapsed().as_micros() as i128;
    let expected_arrival = client_time + offset + latency;
    Packet::Ping {
        client_time,
        expected_arrival,
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
        log::info!("Server linstening on 0.0.0.0:{}", port);
        let socket = server_listen(format!("0.0.0.0:{}", port).as_str(), MAGIC).await;
        log::info!("Connected!");
        let timeout = Duration::new(0, 20_000_000);
        let (send, recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
        let ping_send = send.clone();
        let interval = Duration::new(0, 100_000_000);
        let mut network = Some(NetworkCommunication::new(recv, send, Side::Server));
        loop {
            delay_for(interval).await;
            if ping_send.unbounded_send(ping_packet()).is_err() {
                return;
            }
            if network.is_some() {
                let lock = STATE.lock();
                let state = lock.as_ref().unwrap();
                if state.index == state.time_offset.len() {
                    *NETWORK.lock().unwrap() = Some((
                        network.take().unwrap(),
                        state.start_time.clone(),
                    ));
                }
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
        log::info!("Client connecting...");
        let socket = client_connect("0.0.0.0:0", addr, MAGIC).await;
        log::info!("Client connected!");
        let timeout = Duration::new(0, 20_000_000);
        let (send, recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
        let ping_send = send.clone();
        let interval = Duration::new(0, 100_000_000);
        let mut network = Some(NetworkCommunication::new(recv, send, Side::Client));
        loop {
            delay_for(interval).await;
            if ping_send.unbounded_send(ping_packet()).is_err() {
                return;
            }
            if network.is_some() {
                let lock = STATE.lock();
                let state = lock.as_ref().unwrap();
                if state.index == state.time_offset.len() {
                    // server clock is our reference
                    let now = state.start_time.clone();
                    let duration = state.qin_ding_offset.abs();
                    let duration = Duration::new(
                        (duration / 1_000_000) as u64,
                        (duration % 1_000_000) as u32 * 1000
                    );
                    let now = if state.qin_ding_offset > 0 {
                        now.checked_add(duration)
                    } else {
                        now.checked_sub(duration)
                    }.unwrap();
                    *NETWORK.lock().unwrap() = Some((
                        network.take().unwrap(),
                        now,
                    ));
                }
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
        log::debug!("Start server...");
        create_server_background_loop(port);
        log::debug!("End server");
    });
}

pub fn init_client(addr: String) {
    thread::spawn(move || {
        log::debug!("Start client...");
        create_client_background_loop(&addr);
        log::debug!("End client");
    });
}
