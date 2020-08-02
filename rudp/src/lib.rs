pub mod hand_shake;
mod protocol;
mod receiver;
mod sender;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
pub use protocol::{DeserializeError, PacketDesc};
pub use receiver::BypassResult;
use receiver::{ack_loop, Receiver};
use sender::Sender;
use std::marker::{Send, Sync};
use std::sync::Arc;
use tokio::{net::UdpSocket, select, time::Duration};

async fn udp_loop<
    T: PacketDesc + Send + Sync + 'static,
    F: Fn(T) -> BypassResult<T> + Send + Sync + 'static,
>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    max_retry: u32,
    drop_percentage: u64,
    from_fg: UnboundedReceiver<T>,
    to_bg: Arc<UnboundedSender<T>>,
    to_fg: UnboundedSender<T>,
    bypass: F,
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
        receiver
            .recv_loop(
                &ack_from,
                &to_fg,
                &to_bg,
                max_retry,
                drop_percentage,
                bypass,
            )
            .await;
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
pub fn start_udp_loop<
    T: PacketDesc + Send + Sync + 'static,
    F: Fn(T) -> BypassResult<T> + Send + Sync + 'static,
>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    max_retry: u32,
    bypass: F,
    drop_percentage: u64,
) -> (Arc<UnboundedSender<T>>, UnboundedReceiver<T>) {
    debug_assert!(drop_percentage < 100);
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    let to_background = Arc::new(to_background);
    let to_background_cloned = to_background.clone();
    tokio::spawn(async move {
        udp_loop::<T, _>(
            socket,
            timeout,
            slot_capacity,
            max_retry,
            drop_percentage,
            from_foreground,
            to_background_cloned,
            to_foreground,
            bypass,
        )
        .await;
    });
    (to_background, from_background)
}
