use rudp::{hand_shake::*, start_udp_loop, BypassResult, };
use rudp_derive::PacketDesc;
use std::env;
use std::sync::Mutex;
use lazy_static::lazy_static;
use tokio::{
    join,
    stream::StreamExt,
    time::Duration,
    time::{delay_for, Instant},
};

const MAGIC: &[u8] = "MULTIPONG".as_bytes();

#[derive(serde::Serialize, serde::Deserialize, PacketDesc, PartialEq, Debug)]
enum Packet {
    #[packet(ordered)]
    Ping {
        client_time: i128,
        expected_arrival: i128,
    },
    #[packet(ordered)]
    Pong {
        client_time: i128,
        remote_time: i128,
    }
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
    // actual offset from client in microsecond
    pub actual_offset: i128,
}

lazy_static! {
    static ref STATE: Mutex<State> = Mutex::new(State {
        start_time: Instant::now(),
        time_offset: [0; 16],
        latency: [0; 16],
        qin_ding_offset: 0,
        index: 0,
        actual_offset: 0
    });
}

fn bypass(p: Packet) -> BypassResult<Packet> {
    match p {
        Packet::Ping {client_time, expected_arrival} => {
            let mut lock = STATE.lock();
            let state = lock.as_mut().unwrap();
            let remote_time = state.start_time.elapsed().as_micros() as i128;
            let actual_offset = expected_arrival - remote_time;
            state.actual_offset = actual_offset;
            BypassResult::ToSender(Packet::Pong { client_time, remote_time })
        },
        Packet::Pong { client_time, remote_time } => {
            let mut lock = STATE.lock();
            let state = lock.as_mut().unwrap();
            let now = state.start_time.elapsed().as_micros() as i128;
            let raw_latency = (now - client_time ) / 2;
            let raw_offset = (remote_time - client_time) - raw_latency;

            let index = state.index;
            if index < state.latency.len() {
                state.latency[index] = raw_latency;
                state.time_offset[index] = raw_offset;
                state.index += 1;
                if index == state.latency.len() - 1 {
                    let &offset = state.latency.iter().zip(state.time_offset.iter())
                        .min_by_key(|v| v.0).unwrap().1;
                    state.qin_ding_offset = offset;
                }
            } else {
                let index = state.latency.len() - 1;
                state.latency[index] = raw_latency;
                state.time_offset[index] = raw_offset;
            }
            BypassResult::Discard
        },
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default()
        .filter_or("MY_LOG_LEVEL", "WARN")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);

    let args: Vec<String> = env::args().collect();
    let socket = if args.len() == 2 {
        println!("Waiting for connection...");
        server_listen(&args[1], MAGIC).await
    } else if args.len() == 3 {
        println!("Connecting...");
        client_connect(&args[1], &args[2], MAGIC).await
    } else {
        println!("Usage:");
        println!("Server: <bind address with port>");
        println!("Client: <bind address with port> <server address with port>");
        println!("Note: The bind address should not be 127.0.0.1, better use 0.0.0.0");
        return;
    };
    println!("Connected!");
    // 20ms
    let timeout = Duration::new(0, 20_000_000);
    let (send, mut recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, 10, bypass, 0);
    let recv_task = tokio::spawn(async move {
        loop {
            let p = recv.next().await;
            match p {
                None => {
                    return;
                },
                _ => panic!("should not happen"),
            }
        }
    });
    let send_task = tokio::spawn(async move {
        let interval = Duration::new(0, 100_000_000);
        loop {
            delay_for(interval).await;
            let (start, offset, latency, actual) = {
                let lock = STATE.lock();
                let state = lock.as_ref().unwrap();
                let index = if state.index < state.latency.len() {
                    state.index
                } else {
                    state.latency.len() - 1
                };
                (state.start_time,
                 state.qin_ding_offset,
                 state.latency[index],
                 state.actual_offset)
            };
            println!("Actual: {:>6}, Latency: {:>6}", actual, latency);
            let client_time = start.elapsed().as_micros() as i128;
            let expected_arrival = client_time + offset + latency;
            let packet = Packet::Ping {client_time, expected_arrival};
            if send.unbounded_send(packet).is_err() {
                return;
            }
        }
    });
    let _ = join!(send_task, recv_task);
}
