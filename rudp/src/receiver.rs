use super::{
    protocol::{modify_header, PacketDesc, PacketHeader},
    sender::Sender,
};
use futures::{
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    stream::StreamExt,
};
use log::warn;
use std::{
    collections::HashMap,
    num::Wrapping,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
};
use tokio::{
    net::udp::{RecvHalf, SendHalf},
    sync::{Mutex, Notify},
};

pub struct Receiver {
    inner: RecvHalf,
    slots_generation: Arc<Vec<AtomicI64>>,
    recv_generation: Vec<Option<i64>>,
    slots_used: Arc<Vec<AtomicBool>>,
    notify: Arc<Notify>,
    unreliable_generations: HashMap<u32, i64>,
}

pub enum BypassResult<T> {
    ToSender(T),
    ToUser(T),
    Discard,
}

fn is_new(old: Option<&i64>, current: i64) -> bool {
    if let Some(&old) = old {
        if Wrapping(current) - Wrapping(old) > Wrapping(0) {
            true
        } else {
            false
        }
    } else {
        true
    }
}

impl Receiver {
    pub fn new<T: PacketDesc>(inner: RecvHalf, sender: &Sender<T>) -> Self {
        let slots_generation = sender.get_slots_generation();
        let slots_used = sender.get_slots_used();
        let notify = sender.get_notify();
        let mut recv_generation = Vec::with_capacity(slots_generation.len());
        for _ in 0..slots_generation.len() {
            recv_generation.push(None);
        }
        Receiver {
            inner,
            recv_generation,
            slots_generation,
            slots_used,
            notify,
            unreliable_generations: HashMap::new(),
        }
    }

    /// Handle reliable packet, return true if normal, false if channel closed.
    fn handle_reliable<T: PacketDesc>(&mut self, p: &PacketHeader, data: &[u8]) -> Option<T> {
        if p.slot > self.slots_generation.len() as isize {
            warn!("Received reliable packet with invalid slot ID");
            return None;
        }
        if !is_new(
            self.recv_generation[p.slot as usize - 1].as_ref(),
            p.generation,
        ) {
            return None;
        }
        if T::ordered(p.id) {
            let old = self.unreliable_generations.get(&p.id);
            if is_new(old, p.generation) {
                self.unreliable_generations.insert(p.id, p.generation);
            } else {
                // discard it
                return None;
            }
        }
        let packet = T::deserialize(p.id, data);
        match packet {
            Ok(packet) => {
                self.recv_generation[p.slot as usize - 1] = Some(p.generation);
                Some(packet)
            }
            Err(e) => {
                warn!("Deserialization error: {}", e.0);
                None
            }
        }
    }

    fn handle_unreliable<T: PacketDesc>(&mut self, p: &PacketHeader, data: &[u8]) -> Option<T> {
        if T::ordered(p.id) {
            let old = self.unreliable_generations.get(&p.id);
            if is_new(old, p.generation) {
                self.unreliable_generations.insert(p.id, p.generation);
            } else {
                // discard it
                return None;
            }
        }
        // just receive it
        let packet = T::deserialize(p.id, data);
        match packet {
            Ok(packet) => Some(packet),
            Err(e) => {
                warn!("Deserialization error: {}", e.0);
                None
            }
        }
    }

    fn handle_ack<'a>(&mut self, p: &PacketHeader) {
        // got ACK
        let slot = -p.slot;
        if slot > self.slots_generation.len() as isize {
            warn!("Invalid slot ID for ACK message.");
            return;
        }
        if self.slots_generation[slot as usize - 1].load(Ordering::Acquire) == p.generation {
            if self.slots_used[slot as usize - 1].compare_and_swap(true, false, Ordering::Release) {
                // only notify when it is originally used
                self.notify.notify();
            }
        }
    }

    pub async fn recv_loop<T: PacketDesc, F: Fn(T) -> BypassResult<T>>(
        &mut self,
        ack_channel: &UnboundedSender<(u32, isize, i64)>,
        channel: &UnboundedSender<T>,
        to_sender: &UnboundedSender<T>,
        retry_max: u32,
        drop_percentage: u64,
        bypass: F,
    ) {
        // packet size for UDP is normally 1500 bytes
        const CAPACITY: usize = 1024;
        let mut retry_count = 0;
        let mut recv_buffer = Vec::with_capacity(CAPACITY);
        for _ in 0..CAPACITY {
            recv_buffer.push(0);
        }
        loop {
            let size = self.inner.recv(recv_buffer.as_mut_slice()).await;
            let size = match size {
                Ok(size) if size > 0 => {
                    retry_count = 0;
                    size
                }
                Ok(_) => {
                    warn!("Error receiving data: Payload with length 0.");
                    retry_count += 1;
                    if retry_count == retry_max {
                        return;
                    }
                    continue;
                }
                Err(e) => {
                    warn!("Error receiving data: {}", e.to_string());
                    retry_count += 1;
                    if retry_count == retry_max {
                        return;
                    }
                    continue;
                }
            };
            // simulate packet drop
            if drop_percentage > 0 && rand::random::<u64>() % 100 < drop_percentage {
                continue;
            }
            let result = PacketHeader::deserialize(&recv_buffer[0..size]);
            let (p, data) = match result {
                Ok((p, data)) => (p, data),
                Err(e) => {
                    warn!("Error deserializing header: {}", e.0);
                    continue;
                }
            };
            let p = if p.slot > 0 {
                if ack_channel
                    .unbounded_send((p.id, -p.slot, p.generation))
                    .is_err()
                {
                    None
                } else {
                    self.handle_reliable(&p, &data)
                }
            } else if p.slot == 0 {
                self.handle_unreliable(&p, &data)
            } else {
                self.handle_ack(&p);
                None
            };
            if let Some(p) = p {
                if match bypass(p) {
                    BypassResult::Discard => false,
                    BypassResult::ToSender(p) => to_sender.unbounded_send(p).is_err(),
                    BypassResult::ToUser(p) => channel.unbounded_send(p).is_err(),
                } {
                    break;
                }
            }
        }
    }
}

pub async fn ack_loop(
    send_half: Arc<Mutex<SendHalf>>,
    ack_channel: &mut UnboundedReceiver<(u32, isize, i64)>,
) {
    let mut payload = Vec::new();
    PacketHeader::new(0, 0, 0).serialize(&mut payload);
    while let Some(mut p) = ack_channel.next().await {
        let mut send = send_half.lock().await;
        loop {
            modify_header(&mut payload, p.0, p.1, p.2);
            if let Err(e) = send.send(&payload).await {
                warn!("Error sending ACK: {}", e.to_string());
            }
            if let Ok(next) = ack_channel.try_next() {
                if next.is_none() {
                    return;
                }
                p = next.unwrap();
            } else {
                break;
            }
        }
    }
}
