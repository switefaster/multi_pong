pub mod hand_shake;
mod protocol;
mod receiver;
mod sender;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
pub use protocol::PacketDesc;
use receiver::Receiver;
use sender::Sender;
use std::marker::{Send, Sync};
use std::thread;
use tokio::{join, net::UdpSocket, time::Duration};

#[tokio::main]
async fn udp_loop<T: PacketDesc + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    drop_percentage: u64,
    from_fg: UnboundedReceiver<T>,
    to_fg: UnboundedSender<T>,
) {
    let (recv, send) = socket.split();
    let mut sender = Sender::new(send, timeout, slot_capacity);
    let mut receiver = Receiver::new(recv, &sender);
    let send_task = tokio::spawn(async move {
        let mut from_fg = from_fg;
        sender.send_loop(&mut from_fg).await;
    });
    let recv_task = tokio::spawn(async move {
        let mut to_fg = to_fg;
        receiver.recv_loop(&mut to_fg, drop_percentage).await;
    });
    let _ = join!(send_task, recv_task);
}

pub fn start_udp_loop<T: PacketDesc + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    drop_percentage: u64,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    thread::spawn(move || {
        udp_loop::<T>(
            socket,
            timeout,
            slot_capacity,
            drop_percentage,
            from_foreground,
            to_foreground,
        );
    });
    (to_background, from_background)
}
