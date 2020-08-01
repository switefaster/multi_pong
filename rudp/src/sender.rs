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
    net::udp::SendHalf,
    sync::{Mutex, Notify},
    time::{delay_until, Delay, Duration, Instant},
};

pub struct Sender {
    generation: i64,
    timeout: Duration,
    inner: Arc<Mutex<SendHalf>>,
    slots_generation: Arc<Vec<AtomicI64>>,
    slots_used: Arc<Vec<AtomicBool>>,
    notify: Arc<Notify>,
    queue: VecDeque<Vec<u8>>,
}

struct Slot(Vec<u8>, Instant);

impl Sender {
    pub fn new(inner: SendHalf, timeout: Duration, capacity: usize) -> Self {
        let inner = Arc::new(Mutex::new(inner));
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
            generation: 0,
            timeout,
            inner,
            slots_generation,
            slots_used,
            notify,
            queue: VecDeque::new(),
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

    pub fn get_send_half(&self) -> Arc<Mutex<SendHalf>> {
        self.inner.clone()
    }

    fn get_oldest<'a>(&mut self, slots: &'a mut [Option<Slot>]) -> Option<&'a mut Slot> {
        let mut oldest: Option<&mut Slot> = None;
        for (slot, used) in slots.iter_mut().zip(self.slots_used.iter()) {
            let used = used.load(Ordering::Acquire);
            if used {
                // we are sure that if the slot is used, it must be Some(Slot<T>)
                let time = slot.as_ref().unwrap().1;
                if oldest.is_none() || time < oldest.as_ref().unwrap().1 {
                    oldest = slot.as_mut();
                }
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

    async fn send(&self, buffer: &[u8]) -> bool {
        self.inner.lock().await.send(&buffer).await.is_ok()
    }

    fn put_in<'a>(
        &mut self,
        slots: &'a mut [Option<Slot>],
        mut data: Vec<u8>,
        empty: usize,
    ) -> &'a Vec<u8> {
        let generation = self.generation;
        self.generation += 1;
        self.slots_used[empty].store(true, Ordering::Release);
        self.slots_generation[empty].store(generation, Ordering::Release);
        modify_header(&mut data, empty as isize + 1, generation);
        slots[empty] = Some(Slot(data, Instant::now()));
        &slots[empty].as_ref().unwrap().0
    }

    fn resend<'a>(&mut self, slots: &'a mut [Option<Slot>]) -> Option<&'a Vec<u8>> {
        let timeout = self.timeout;
        let oldest = self.get_oldest(slots);
        match oldest {
            Some(mut oldest) if oldest.1.elapsed() > timeout => {
                oldest.1 = Instant::now();
                Some(&oldest.0)
            }
            _ => None,
        }
    }

    async fn queue<T: PacketDesc>(&mut self, data: T) -> Option<Vec<u8>> {
        if data.reliable() {
            // slot and generation are just dummy value, would be set to the actual value when we
            // call `put_in`
            let mut payload = PacketHeader::new(data.id(), 0, 0).serialize();
            data.serialize(&mut payload);
            self.queue.push_back(payload);
            None
        } else {
            let generation = self.generation;
            self.generation += 1;
            let mut payload = PacketHeader::new(data.id(), 0, generation).serialize();
            data.serialize(&mut payload);
            Some(payload)
        }
    }

    pub async fn send_loop<T: PacketDesc>(&mut self, channel: &mut UnboundedReceiver<T>) {
        let mut slots = Vec::with_capacity(self.slots_used.len());
        for _ in 0..self.slots_used.len() {
            slots.push(None);
        }
        let timeout = Fuse::<Delay>::terminated();
        let notify = self.get_notify();
        let ack = notify.notified().fuse();
        let receive = channel.into_future().fuse();
        pin_mut!(timeout, ack, receive);
        loop {
            select_biased! {
                _ = timeout => (),
                _ = ack => {
                    ack.set(notify.notified().fuse());
                },
                (mut item, stream) = receive => {
                    loop {
                        match item {
                            Some(p) => if let Some(p) = self.queue(p).await {
                                if !self.send(&p).await {
                                    return;
                                }
                            },
                            None => {
                                return;
                            }
                        }
                        if let Ok(p) = stream.try_next() {
                            item = p;
                        } else {
                            break;
                        }
                    }
                    receive.set(stream.into_future().fuse());
                },
            };
            if timeout.is_terminated() {
                if let Some(slot) = self.get_oldest(&mut slots) {
                    let deadline = slot.1 + self.timeout;
                    timeout.set(delay_until(deadline).fuse());
                }
            }

            // resend all timeout packets
            while let Some(p) = self.resend(&mut slots) {
                if !self.send(&p).await {
                    return;
                }
            }
            // send all packets in queue if there is some slot which is empty...
            while let Some(empty) = self.find_empty_slot() {
                let p = self.queue.pop_front();
                if let Some(p) = p {
                    let p = self.put_in(&mut slots, p, empty).clone();
                    if !self.send(&p).await {
                        return;
                    }
                } else {
                    break;
                }
            }
        }
    }
}
