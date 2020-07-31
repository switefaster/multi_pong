use rudp::{hand_shake::*, start_udp_loop, PacketDesc, BypassResult};
use std::convert::TryInto;
use std::env;
use std::mem::size_of;
use std::rc::Rc;
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

    fn data(&self) -> Rc<Vec<u8>> {
        let (a, b, c) = match *self {
            Packet::Ping(a, b, c) => (a, b, c),
            Packet::Pong(a, b, c) => (a, b, c),
        };
        let mut v: Vec<u8> = Vec::new();
        v.push(a as u8);
        v.extend(b.to_be_bytes().iter());
        v.extend(c.to_be_bytes().iter());
        Rc::new(v)
    }

    fn deserialize(id: u32, data: &[u8]) -> Self {
        let a = data[0] == 1;
        let b = u64::from_be_bytes(data[1..size_of::<u64>() + 1].try_into().unwrap());
        let c = u128::from_be_bytes(
            data[size_of::<u64>() + 1..size_of::<u128>() + size_of::<u64>() + 1]
                .try_into()
                .unwrap(),
        );
        match id {
            0 => Packet::Ping(a, b, c),
            1 => Packet::Pong(a, b, c),
            _ => panic!("Invalid ID!")
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

fn bypass(p: Packet) -> BypassResult<Packet> {
    match p {
        Packet::Ping(a, b, c) => BypassResult::ToSender(Packet::Pong(a, b, c)),
        _ => BypassResult::ToUser(p),
    }
}

#[tokio::main]
async fn main() {
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
    let (send, mut recv) = start_udp_loop::<Packet, _>(socket, timeout, 10, bypass, 0);
    let start = Instant::now();
    let recv_task = tokio::spawn(async move {
        loop {
            let p = recv.next().await;
            match p {
                Some(Packet::Ping(_, _, _)) => {
                    panic!("Should not happen!");
                }
                Some(Packet::Pong(reliable, id, timestamp)) => {
                    println!(
                        "ID: {}, time: {}ms, reliable: {}",
                        id,
                        (start.elapsed().as_micros() - timestamp) / 1000,
                        reliable
                    );
                },
                None => {
                    return;
                }
            }
        }
    });
    let send_task = tokio::spawn(async move {
        let mut id = 0;
        let interval = Duration::new(0, 500_000_000);
        loop {
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
