use super::{
    protocol::{Packet, PacketDesc},
    sender::Sender,
};
use futures::channel::mpsc::UnboundedSender;
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
    send_half: Arc<Mutex<SendHalf>>,
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
    pub fn new(inner: RecvHalf, sender: &Sender) -> Self {
        let send_half = sender.get_send_half();
        let slots_generation = sender.get_slots_generation();
        let slots_used = sender.get_slots_used();
        let notify = sender.get_notify();
        let mut recv_generation = Vec::with_capacity(slots_generation.len());
        for _ in 0..slots_generation.len() {
            recv_generation.push(None);
        }
        Receiver {
            inner,
            send_half,
            recv_generation,
            slots_generation,
            slots_used,
            notify,
            unreliable_generations: HashMap::new(),
        }
    }

    /// Handle reliable packet, return true if normal, false if channel closed.
    async fn handle_reliable<'a, T: PacketDesc>(
        &mut self,
        p: &Packet<'a>,
    ) -> Option<T> {
        assert!(p.slot <= self.slots_generation.len() as isize);
        let new = is_new(
            self.recv_generation[p.slot as usize - 1].as_ref(),
            p.generation,
        );
        // we are assuming this takes a very short time...
        // actually we can send a dummy message, but I don't want to change the type of
        // DataPacket right now...
        self.send_half
            .lock()
            .await
            .send(&Packet::new(&[], p.id, -p.slot, p.generation).serialize())
            .await
            .unwrap();
        if new {
            self.recv_generation[p.slot as usize - 1] = Some(p.generation);
            Some(T::deserialize(p.id, p.data))
        } else {
            None
        }
    }

    fn handle_unreliable<'a, T: PacketDesc>(
        &mut self,
        p: &Packet<'a>,
    ) -> Option<T> {
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
        Some(T::deserialize(p.id, p.data))
    }

    fn handle_ack<'a>(&mut self, p: &Packet<'a>) {
        // got ACK
        let slot = -p.slot;
        assert!(slot <= self.slots_generation.len() as isize);
        if self.slots_generation[slot as usize - 1].load(Ordering::Acquire) == p.generation {
            if self.slots_used[slot as usize - 1].compare_and_swap(true, false, Ordering::Release) {
                // only notify when it is originally used
                self.notify.notify();
            }
        }
    }

    pub async fn recv_loop<T: PacketDesc, F: Fn(T) -> BypassResult<T>>(
        &mut self,
        channel: &UnboundedSender<T>,
        to_sender: &UnboundedSender<T>,
        bypass: F,
        drop_percentage: u64,
    ) {
        // packet size for UDP is normally 1500 bytes
        const CAPACITY: usize = 1024;
        let mut recv_buffer = Vec::with_capacity(CAPACITY);
        for _ in 0..CAPACITY {
            recv_buffer.push(0);
        }
        loop {
            let size = self.inner.recv(recv_buffer.as_mut_slice()).await;
            if size.is_err() {
                return;
            }
            // simulate packet drop
            if drop_percentage > 0 && rand::random::<u64>() % 100 < drop_percentage {
                continue;
            }
            let size = size.unwrap();
            let p = Packet::deserialize(&recv_buffer[0..size]);
            let p = if p.slot > 0 {
                self.handle_reliable(&p).await
            } else if p.slot == 0 {
                self.handle_unreliable(&p)
            } else {
                self.handle_ack(&p);
                None
            };
            if let Some(p) = p {
                if match bypass(p) {
                    BypassResult::ToSender(p) => to_sender.unbounded_send(p).is_err(),
                    BypassResult::ToUser(p) => channel.unbounded_send(p).is_err(),
                    BypassResult::Discard => false
                } {
                    break;
                }
            }
        }
    }
}
