use futures::{channel::mpsc::unbounded, future::FutureExt, pin_mut, select};
use multi_pong::network::udp;
use serde::{Deserialize, Serialize};
use std::env;
use tokio::{
    join,
    net::UdpSocket,
    stream::StreamExt,
    time::Duration,
    time::{delay_for, Instant},
};

const MAGIC: &[u8] = "MULTIPONG".as_bytes();

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Packet {
    Ping(bool, u64, u128),
    Pong(bool, u64, u128),
}

async fn server_listen(bind: &str) -> UdpSocket {
    const CAPACITY: usize = 2048;
    let mut buffer: Vec<u8> = Vec::with_capacity(CAPACITY);
    for _ in 0..CAPACITY {
        buffer.push(0);
    }
    let mut socket = UdpSocket::bind(bind).await.unwrap();
    loop {
        let (len, from) = socket.recv_from(buffer.as_mut_slice()).await.unwrap();
        if len == MAGIC.len() && &buffer[..MAGIC.len()] == MAGIC {
            socket.connect(from).await.unwrap();
            break;
        }
    }
    // send magic back to client to notify connection established,
    // and wait until the client send something different
    while &buffer[..MAGIC.len()] == MAGIC {
        socket.send(MAGIC).await.unwrap();
        let _ = socket.recv(buffer.as_mut_slice()).await.unwrap();
    }
    socket
}

async fn client_connect(bind: &str, server: &str) -> UdpSocket {
    let timeout = Duration::new(0, 100_000_000);
    let mut buffer: Vec<u8> = Vec::with_capacity(MAGIC.len());
    for _ in MAGIC.iter() {
        buffer.push(0);
    }
    let socket = UdpSocket::bind(bind).await.unwrap();
    socket.connect(server).await.unwrap();
    let (mut recv, mut send) = socket.split();
    loop {
        send.send(MAGIC).await.unwrap();
        select! {
            _ = recv.recv(buffer.as_mut_slice()).fuse() => {
                if buffer == MAGIC {
                    break;
                }
            },
            _ = delay_for(timeout).fuse() => {
            }
        }
    }
    send.reunite(recv).unwrap()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let socket = if args.len() == 2 {
        println!("Waiting for connection...");
        server_listen(&args[1]).await
    } else if args.len() == 3 {
        println!("Connecting...");
        client_connect(&args[1], &args[2]).await
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
    let (send, mut recv) = udp::start_udp_loop::<Packet>(socket, timeout, 10, 0);
    let start = Instant::now();
    let (ack_send, mut ack_recv) = unbounded::<udp::PacketType<Packet>>();
    let recv_task = tokio::spawn(async move {
        loop {
            let p = recv.next().await.unwrap();
            match p.data {
                Packet::Ping(reliable, id, timestamp) => {
                    let packet = Packet::Pong(reliable, id, timestamp);
                    let packet = if reliable {
                        udp::PacketType::Reliable(packet)
                    } else {
                        udp::PacketType::Unreliable(packet)
                    };
                    ack_send.unbounded_send(packet).unwrap();
                }
                Packet::Pong(reliable, id, timestamp) => {
                    println!(
                        "ID: {}, time: {}ms, reliable: {}",
                        id,
                        (start.elapsed().as_micros() - timestamp)/1000,
                        reliable
                    );
                }
            }
        }
    });
    let send_task = tokio::spawn(async move {
        let mut id = 0;
        let interval = Duration::new(0, 500_000_000);
        let interval_future = delay_for(interval).fuse();
        pin_mut!(interval_future);
        loop {
            select! {
                p = ack_recv.next().fuse() => {
                    send.unbounded_send(p.unwrap()).unwrap();
                },
                _ = interval_future => {
                    interval_future.set(delay_for(interval).fuse());
                    for _ in 0..5 {
                        let reliable = id % 5 == 0;
                        let packet = Packet::Ping(reliable, id, start.elapsed().as_micros());
                        id += 1;
                        let packet = if reliable {
                            udp::PacketType::Reliable(packet)
                        } else {
                            udp::PacketType::Unreliable(packet)
                        };
                        send.unbounded_send(packet).unwrap();
                    }
                }
            }
        }
    });
    let _ = join!(send_task, recv_task);
}

