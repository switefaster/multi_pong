use tokio::{
    net::UdpSocket,
    time::Duration,
    time::delay_for,
};

use futures::{future::FutureExt, select};

pub async fn server_listen(bind: &str, magic: &[u8]) -> UdpSocket {
    const CAPACITY: usize = 2048;
    let mut buffer: Vec<u8> = Vec::with_capacity(CAPACITY);
    for _ in 0..CAPACITY {
        buffer.push(0);
    }
    let mut socket = UdpSocket::bind(bind).await.unwrap();
    loop {
        let (len, from) = socket.recv_from(buffer.as_mut_slice()).await.unwrap();
        if len == magic.len() && &buffer[..magic.len()] == magic {
            socket.connect(from).await.unwrap();
            break;
        }
    }
    // send magic back to client to notify connection established,
    // and wait until the client send something different
    while &buffer[..magic.len()] == magic {
        socket.send(magic).await.unwrap();
        let _ = socket.recv(buffer.as_mut_slice()).await.unwrap();
    }
    socket
}

pub async fn client_connect(bind: &str, server: &str, magic: &[u8]) -> UdpSocket {
    let timeout = Duration::new(0, 100_000_000);
    let mut buffer: Vec<u8> = Vec::with_capacity(magic.len());
    for _ in magic.iter() {
        buffer.push(0);
    }
    let socket = UdpSocket::bind(bind).await.unwrap();
    socket.connect(server).await.unwrap();
    let (mut recv, mut send) = socket.split();
    loop {
        send.send(magic).await.unwrap();
        select! {
            _ = recv.recv(buffer.as_mut_slice()).fuse() => {
                if buffer == magic {
                    break;
                }
            },
            _ = delay_for(timeout).fuse() => {
            }
        }
    }
    send.reunite(recv).unwrap()
}

