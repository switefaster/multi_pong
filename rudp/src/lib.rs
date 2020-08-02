pub mod hand_shake;
mod protocol;
mod receiver;
mod sender;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
pub use protocol::{DeserializeError, PacketDesc};
use receiver::{Receiver, ack_loop};
use sender::Sender;
use std::marker::{Send, Sync};
use tokio::{net::UdpSocket, select, time::Duration};

async fn udp_loop<T: PacketDesc + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    max_retry: u32,
    drop_percentage: u64,
    from_fg: UnboundedReceiver<T>,
    to_fg: UnboundedSender<T>,
) {
    let (ack_from, mut ack_to) = unbounded();
    let (recv, send) = socket.split();
    let mut sender = Sender::<T>::new(send, timeout, slot_capacity, max_retry);
    let send_half = sender.get_send_half();
    let mut receiver = Receiver::new(recv, &sender);
    let send_task = tokio::spawn(async move {
        let mut from_fg = from_fg;
        sender.send_loop(&mut from_fg).await;
    });
    let recv_task = tokio::spawn(async move {
        let to_fg = to_fg;
        receiver.recv_loop(&ack_from, &to_fg, max_retry, drop_percentage).await;
    });
    let ack_task = tokio::spawn(async move {
        ack_loop(send_half, &mut ack_to).await;
    });
    // Close the task when any finishes.
    select!(
        _ = send_task => (),
        _ = recv_task => (),
        _ = ack_task => ()
    );
}

/// Start the UDP loop.
/// # Parameters
/// * socket: Socket for communication, should be connected already.
/// * timeout: Timeout for retransmission.
/// * slot_capacity: Number of slots for sending reliable packets *in parallel*.
/// * max_retry: Maximum number of consecutive send/recv attempts when the socket failed to work.
///   If reached, the respective task would exit. Note that this is not resend attempt.
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
    tokio::spawn(async move {
        udp_loop::<T>(
            socket,
            timeout,
            slot_capacity,
            max_retry,
            drop_percentage,
            from_foreground,
            to_foreground,
        ).await;
    });
    (to_background, from_background)
}
