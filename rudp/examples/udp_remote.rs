use rudp::{hand_shake::*, start_udp_loop, DeserializeError, PacketDesc};
use std::convert::TryInto;
use std::env;
use std::mem::size_of;
use std::sync::Arc;
use tokio::{
    join,
    stream::StreamExt,
    time::Duration,
    time::{delay_for, Instant},
};

const MAGIC: &[u8] = "MULTIPONG".as_bytes();

#[derive(Debug, Clone)]
enum Packet {
    Ping(bool, u64, u128),
    Pong(bool, u64, u128),
}

impl PacketDesc for Packet {
    fn id(&self) -> u32 {
        match *self {
            Packet::Ping(_, _, _) => 0,
            Packet::Pong(_, _, _) => 1,
        }
    }

    fn serialize(&self, writer: &mut Vec<u8>) {
        let (a, b, c) = match *self {
            Packet::Ping(a, b, c) => (a, b, c),
            Packet::Pong(a, b, c) => (a, b, c),
        };
        writer.push(a as u8);
        writer.extend(b.to_be_bytes().iter());
        writer.extend(c.to_be_bytes().iter());
    }

    fn deserialize(id: u32, data: &[u8]) -> Result<Self, DeserializeError> {
        let a = data[0] == 1;
        let b = u64::from_be_bytes(data[1..size_of::<u64>() + 1].try_into().map_err(|_| {
            DeserializeError("Error deserializing message index (Packet.1)".to_string())
        })?);
        let c = u128::from_be_bytes(
            data[size_of::<u64>() + 1..size_of::<u128>() + size_of::<u64>() + 1]
                .try_into()
                .map_err(|_| {
                    DeserializeError("Error deserializing timestamp (Packet.2)".to_string())
                })?,
        );
        match id {
            0 => Ok(Packet::Ping(a, b, c)),
            1 => Ok(Packet::Pong(a, b, c)),
            _ => Err(DeserializeError("Invalid ID!".to_string())),
        }
    }

    fn reliable(&self) -> bool {
        match *self {
            Packet::Ping(a, _, _) => a,
            Packet::Pong(a, _, _) => a,
        }
    }

    fn ordered(_: u32) -> bool {
        false
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
    let (send, mut recv) = start_udp_loop::<Packet>(socket, timeout, 10, 10, 0);
    let send = Arc::new(send);
    let send_from_recv = send.clone();
    let start = Instant::now();
    let recv_task = tokio::spawn(async move {
        loop {
            let p = recv.next().await;
            match p {
                Some(Packet::Ping(reliable, id, timestamp)) => {
                    let packet = Packet::Pong(reliable, id, timestamp);
                    send_from_recv.unbounded_send(packet).unwrap();
                }
                Some(Packet::Pong(reliable, id, timestamp)) => {
                    println!(
                        "ID: {}, time: {}µs, reliable: {}",
                        id,
                        (start.elapsed().as_micros() - timestamp),
                        reliable
                    );
                }
                None => {
                    return;
                }
            }
        }
    });
    let send_task = tokio::spawn(async move {
        let mut id = 0;
        let interval = Duration::new(0, 5_000_000);
        for _ in 0..1000 {
            delay_for(interval).await;
            for _ in 0..5 {
                let reliable = id % 5 == 0;
                let packet = Packet::Ping(reliable, id, start.elapsed().as_micros());
                id += 1;
                if send.unbounded_send(packet).is_err() {
                    return;
                }
            }
        }
    });
    let _ = join!(send_task, recv_task);
}
