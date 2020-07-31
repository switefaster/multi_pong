pub mod hand_shake;
mod protocol;
mod receiver;
mod sender;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
pub use protocol::PacketDesc;
pub use receiver::BypassResult;
use receiver::Receiver;
use sender::Sender;
use std::marker::{Send, Sync};
use std::thread;
use std::sync::Arc;
use tokio::{join, net::UdpSocket, time::Duration};

#[tokio::main]
async fn udp_loop<
    T: PacketDesc + Send + Sync + 'static,
    F: Fn(T) -> BypassResult<T> + Send + Sync + 'static,
>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    drop_percentage: u64,
    from_fg: UnboundedReceiver<T>,
    to_bg: Arc<UnboundedSender<T>>,
    to_fg: UnboundedSender<T>,
    bypass: F,
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
        receiver
            .recv_loop(&mut to_fg, &to_bg, bypass, drop_percentage)
            .await;
    });
    let _ = join!(send_task, recv_task);
}

pub fn start_udp_loop<T: PacketDesc + Send + Sync + 'static, F: Fn(T) -> BypassResult<T> + Send + Sync + 'static> (
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    bypass: F,
    drop_percentage: u64,
) -> (Arc<UnboundedSender<T>>, UnboundedReceiver<T>) {
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    let to_background = Arc::new(to_background);
    let to_background_cloned = to_background.clone();
    thread::spawn(move || {
        udp_loop::<T, _>(
            socket,
            timeout,
            slot_capacity,
            drop_percentage,
            from_foreground,
            to_background_cloned,
            to_foreground,
            bypass
        );
    });
    (to_background, from_background)
}
