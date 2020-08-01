pub mod hand_shake;
mod protocol;
mod receiver;
mod sender;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
pub use protocol::{DeserializeError, PacketDesc};
use receiver::Receiver;
use sender::Sender;
use std::marker::{Send, Sync};
use std::thread;
use tokio::{net::UdpSocket, select, time::Duration};

#[tokio::main]
async fn udp_loop<T: PacketDesc + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    max_retry: u32,
    drop_percentage: u64,
    from_fg: UnboundedReceiver<T>,
    to_fg: UnboundedSender<T>,
) {
    let (recv, send) = socket.split();
    let mut sender = Sender::new(send, timeout, slot_capacity, max_retry);
    let mut receiver = Receiver::new(recv, &sender);
    let send_task = tokio::spawn(async move {
        let mut from_fg = from_fg;
        sender.send_loop(&mut from_fg).await;
    });
    let recv_task = tokio::spawn(async move {
        let mut to_fg = to_fg;
        receiver.recv_loop(&mut to_fg, max_retry, drop_percentage).await;
    });
    // Close the task when any finishes.
    select!(
        _ = send_task => (),
        _ = recv_task => ()
    );
}

/// Start the UDP loop.
/// # Parameters
/// * socket: Socket for communication, should be connected already.
/// * timeout: Timeout for retransmission.
/// * slot_capacity: Number of slots for sending reliable packets *in parallel*.
/// * max_retry: Maximum number of consecutive recv attempts when the socket failed to receive messages.
///   If reached, the receive task would exit. Note that this is not resend attempt.
/// * drop_percentage: Packet drop rate for simulating packet drop. If 0, it would not attemp to
///   simulate packet drop. Should be within 0..100. Note that the probability is not really that
///   accurate, this is for testing only.
pub fn start_udp_loop<T: PacketDesc + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    max_retry: u32,
    drop_percentage: u64,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    debug_assert!(drop_percentage < 100);
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    thread::spawn(move || {
        udp_loop::<T>(
            socket,
            timeout,
            slot_capacity,
            max_retry,
            drop_percentage,
            from_foreground,
            to_foreground,
        );
    });
    (to_background, from_background)
}
