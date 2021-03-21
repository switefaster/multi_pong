use super::protocol::{modify_header, PacketDesc, PacketHeader};
use futures::{
    channel::mpsc::UnboundedReceiver,
    future::{Fuse, FusedFuture, FutureExt},
    pin_mut, select_biased,
    stream::StreamExt,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
};
use tokio::{
    net::UdpSocket,
    sync::Notify,
    time::{sleep_until, Duration, Instant, Sleep},
};

pub struct Sender<T: PacketDesc> {
    retry_count: u32,
    retry_max: u32,
    generation: i64,
    timeout: Duration,
    inner: Arc<UdpSocket>,
    slots_generation: Arc<Vec<AtomicI64>>,
    slots_used: Arc<Vec<AtomicBool>>,
    notify: Arc<Notify>,
    queue: VecDeque<T>,
    used_queue: VecDeque<usize>,
}

struct Slot(Vec<u8>, Instant);

impl<T: PacketDesc> Sender<T> {
    pub fn new(inner: Arc<UdpSocket>, timeout: Duration, capacity: usize, retry_max: u32) -> Self {
        let mut slots_generation = Vec::with_capacity(capacity);
        let mut slots_used = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots_generation.push(AtomicI64::new(0));
            slots_used.push(AtomicBool::new(false));
        }
        let slots_generation = Arc::new(slots_generation);
        let slots_used = Arc::new(slots_used);
        let notify = Arc::new(Notify::new());
        Sender {
            retry_count: 0,
            retry_max,
            generation: 0,
            timeout,
            inner,
            slots_generation,
            slots_used,
            notify,
            queue: VecDeque::new(),
            used_queue: VecDeque::with_capacity(capacity),
        }
    }

    pub fn get_notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    pub fn get_slots_generation(&self) -> Arc<Vec<AtomicI64>> {
        self.slots_generation.clone()
    }

    pub fn get_slots_used(&self) -> Arc<Vec<AtomicBool>> {
        self.slots_used.clone()
    }

    fn get_oldest<'a>(&mut self, slots: &'a mut [Slot]) -> Option<&'a mut Slot> {
        let mut oldest: Option<&mut Slot> = None;
        while let Some(&i) = self.used_queue.front() {
            let used = self.slots_used[i].load(Ordering::Relaxed);
            if used {
                oldest = Some(&mut slots[i]);
                break;
            } else {
                self.used_queue.pop_front();
            }
        }
        oldest
    }

    fn find_empty_slot(&self) -> Option<usize> {
        let mut empty = None;
        for i in 0..self.slots_used.len() {
            let used = self.slots_used[i].load(Ordering::Acquire);
            if !used {
                empty = Some(i);
                break;
            }
        }
        empty
    }

    /// Attempt to send the buffer once, return false if send continuously failed. (reaches the max retry)
    async fn send(&mut self, buffer: &[u8]) -> bool {
        if self.inner.send(&buffer).await.is_ok() {
            self.retry_count = 0;
            true
        } else {
            self.retry_count += 1;
            self.retry_count != self.retry_max
        }
    }

    fn put_in<'a>(&mut self, slots: &'a mut [Slot], data: T, empty: usize) -> &'a Vec<u8> {
        let generation = self.generation;
        self.generation += 1;
        self.slots_used[empty].store(true, Ordering::Relaxed);
        self.slots_generation[empty].store(generation, Ordering::Release);
        self.used_queue.push_back(empty);
        slots[empty].0.clear();
        PacketHeader::new(data.id(), empty as isize + 1, generation).serialize(&mut slots[empty].0);
        data.serialize(&mut slots[empty].0);
        &slots[empty].0
    }

    fn resend<'a>(&mut self, slots: &'a mut [Slot]) -> Option<&'a Vec<u8>> {
        let timeout = self.timeout;
        let oldest = self.get_oldest(slots);
        match oldest {
            Some(mut oldest) if oldest.1.elapsed() > timeout => {
                let index = self.used_queue.pop_front().unwrap();
                self.used_queue.push_back(index);
                oldest.1 = Instant::now();
                Some(&oldest.0)
            }
            _ => None,
        }
    }

    async fn queue(&mut self, data: T) -> Option<T> {
        if data.reliable() {
            // slot and generation are just dummy value, would be set to the actual value when we
            // call `put_in`
            self.queue.push_back(data);
            None
        } else {
            Some(data)
        }
    }

    fn prepare_unreliable<'a>(&mut self, payload: &'a mut Vec<u8>, packet: &T) -> &'a [u8] {
        let generation = self.generation;
        self.generation += 1;
        payload.clear();
        PacketHeader::new(packet.id(), 0, generation).serialize(payload);
        packet.serialize(payload);
        payload
    }

    pub async fn send_loop(
        &mut self,
        channel: &mut UnboundedReceiver<T>,
        ack_channel: &mut UnboundedReceiver<(u32, isize, i64)>,
    ) {
        let mut slots = Vec::with_capacity(self.slots_used.len());
        let now = Instant::now();
        for _ in 0..self.slots_used.len() {
            slots.push(Slot(Vec::with_capacity(100), now));
        }
        let mut ack_payload = Vec::new();
        PacketHeader::new(0, 0, 0).serialize(&mut ack_payload);
        let mut unreliable_payload = Vec::with_capacity(100);

        let timeout = Fuse::<Sleep>::terminated();
        let notify = self.get_notify();
        let got_ack = notify.notified().fuse();
        let receive = channel.into_future().fuse();
        let ack_receive = ack_channel.into_future().fuse();
        pin_mut!(timeout, got_ack, receive, ack_receive);
        loop {
            if timeout.is_terminated() {
                if let Some(slot) = self.get_oldest(&mut slots) {
                    let deadline = slot.1 + self.timeout;
                    timeout.set(sleep_until(deadline).fuse());
                }
            }
            select_biased! {
                _ = timeout => (),
                _ = got_ack => {
                    got_ack.set(notify.notified().fuse());
                },
                (mut item, stream) = receive => {
                    receive.set(stream.into_future().fuse());
                    match item {
                        Some(p) => if let Some(p) = self.queue(p).await {
                            let payload = self.prepare_unreliable(&mut unreliable_payload, &p);
                            if !self.send(payload).await {
                                return;
                            }
                            continue;
                        },
                        None => {
                            return;
                        }
                    }
                },
                (p, stream) = ack_receive => {
                    ack_receive.set(stream.into_future().fuse());
                    match p {
                        Some(p) => {
                            modify_header(&mut ack_payload, p.0, p.1, p.2);
                            if !self.send(&ack_payload).await {
                                return;
                            }
                            continue;
                        },
                        None => {
                            return;
                        }
                    }
                }
            };

            // resend all timeout packets
            if let Some(p) = self.resend(&mut slots) {
                if !self.send(&p).await {
                    return;
                }
                continue;
            }
            // send all packets in queue if there is some slot which is empty...
            while let Some(empty) = self.find_empty_slot() {
                let p = self.queue.pop_front();
                if let Some(p) = p {
                    let p = self.put_in(&mut slots, p, empty).clone();
                    if !self.send(&p).await {
                        return;
                    }
                    break;
                } else {
                    break;
                }
            }
        }
    }
}
