use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    future::{Fuse, FutureExt, FusedFuture},
    pin_mut, select_biased, join,
    stream::StreamExt,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_cbor::{from_slice, to_vec};
use std::collections::VecDeque;
use std::marker::{Send, Sync};
use std::num::Wrapping;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::{
    net::{
        udp::{RecvHalf, SendHalf},
        UdpSocket,
    },
    sync::{Mutex, Notify},
    time::{delay_until, Delay, Duration, Instant},
};

pub enum PacketType<T> {
    Reliable(T),
    Unreliable(T),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DataPacket<T> {
    // slot > 0: require ACK, stored in slots[slot - 1]
    // slot = 0: unreliable packet
    // slot < 0: is ACK, stored in slots[-slot - 1]
    pub slot: isize,
    pub generation: i64,
    pub data: T,
}

struct Slot<T>(DataPacket<T>, Instant);

/// Sender struct
struct Sender<T> {
    generation: i64,
    timeout: Duration,
    inner: Arc<Mutex<SendHalf>>,
    slots: Vec<Option<Slot<T>>>,
    slots_generation: Arc<Vec<AtomicI64>>,
    slots_used: Arc<Vec<AtomicBool>>,
    notify: Arc<Notify>,
    queue: VecDeque<T>,
    temp: Option<DataPacket<T>>,
}

impl<T: Serialize + Clone> Sender<T> {
    pub fn new(inner: SendHalf, timeout: Duration, capacity: usize) -> Self {
        let inner = Arc::new(Mutex::new(inner));
        let mut slots = Vec::with_capacity(capacity);
        let mut slots_generation = Vec::with_capacity(capacity);
        let mut slots_used = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(None);
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
            slots,
            slots_generation,
            slots_used,
            notify,
            queue: VecDeque::new(),
            temp: None,
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

    fn get_oldest(&mut self) -> Option<&mut Slot<T>> {
        let mut oldest: Option<&mut Slot<T>> = None;
        for (slot, used) in self.slots.iter_mut().zip(self.slots_used.iter()) {
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

    async fn send(&mut self, value: &DataPacket<T>) {
        let buffer = to_vec(value).unwrap();
        self.inner.lock().await.send(&buffer).await.unwrap();
    }

    fn put_in(&mut self, data: T, empty: usize) -> &DataPacket<T> {
        let generation = self.generation;
        self.generation += 1;
        self.slots_used[empty].store(true, Ordering::Release);
        self.slots_generation[empty].store(generation, Ordering::Release);
        self.slots[empty] = Some(Slot(
            DataPacket {
                slot: empty as isize + 1,
                generation,
                data,
            },
            Instant::now(),
        ));
        &self.slots[empty].as_ref().unwrap().0
    }

    fn resend(&mut self) -> Option<&DataPacket<T>> {
        let timeout = self.timeout;
        let oldest = self.get_oldest();
        match oldest {
            Some(mut oldest) if oldest.1.elapsed() > timeout => {
                oldest.1 = Instant::now();
                Some(&oldest.0)
            }
            _ => None,
        }
    }

    async fn queue(&mut self, packet: PacketType<T>) -> Option<&DataPacket<T>> {
        match packet {
            PacketType::Reliable(data) => {
                self.queue.push_back(data);
                None
            }
            PacketType::Unreliable(data) => {
                let generation = self.generation;
                self.generation += 1;
                self.temp = Some(DataPacket {
                    slot: 0,
                    generation,
                    data,
                });
                Some(&self.temp.as_ref().unwrap())
            }
        }
    }

    pub async fn send_loop(&mut self, channel: &mut UnboundedReceiver<PacketType<T>>) {
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
                                let p = p.clone();
                                self.send(&p).await;
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
            // FIXME: A lot of copying going on for this scheme
            // probably we can fix it by pinning

            if timeout.is_terminated() {
                if let Some(slot) = self.get_oldest() {
                    let deadline = slot.1 + self.timeout;
                    timeout.set(delay_until(deadline).fuse());
                }
            }

            // resend all timeout packets
            while let Some(p) = self.resend() {
                let p = p.clone();
                self.send(&p).await;
            }
            // send all packets in queue if there is some slot which is empty...
            while let Some(empty) = self.find_empty_slot() {
                let p = self.queue.pop_front();
                if let Some(p) = p {
                    let p = self.put_in(p, empty).clone();
                    self.send(&p).await;
                } else {
                    break;
                }
            }
        }
    }
}

struct Receiver {
    inner: RecvHalf,
    send_half: Arc<Mutex<SendHalf>>,
    slots_generation: Arc<Vec<AtomicI64>>,
    recv_generation: Vec<Option<i64>>,
    slots_used: Arc<Vec<AtomicBool>>,
    notify: Arc<Notify>,
}

impl Receiver {
    pub fn new<T2: Serialize + Clone>(inner: RecvHalf, sender: &Sender<T2>) -> Self {
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
        }
    }

    pub async fn recv_loop<T: Serialize + DeserializeOwned>(
        &mut self,
        channel: &mut UnboundedSender<DataPacket<T>>,
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
                break;
            }
            // simulate packet drop
            if drop_percentage > 0 && rand::random::<u64>() % 100 < drop_percentage {
                continue;
            }
            let size = size.unwrap();
            let mut p = from_slice::<DataPacket<T>>(&recv_buffer[0..size]).unwrap();
            if p.slot > 0 {
                assert!(p.slot <= self.slots_generation.len() as isize);
                let current = self.recv_generation[p.slot as usize - 1];
                let new = if let Some(current) = current {
                    if Wrapping(p.generation) - Wrapping(current) <= Wrapping(0) {
                        false
                    } else {
                        true
                    }
                } else {
                    true
                };
                // send ACK
                p.slot = -p.slot;
                // we are assuming this takes a very short time...
                // actually we can send a dummy message, but I don't want to change the type of
                // DataPacket right now...
                self.send_half
                    .lock()
                    .await
                    .send(&to_vec(&p).unwrap())
                    .await
                    .unwrap();
                p.slot = -p.slot;
                if new {
                    self.recv_generation[p.slot as usize - 1] = Some(p.generation);
                    channel.unbounded_send(p).unwrap();
                }
            } else if p.slot == 0 {
                // just receive it
                channel.unbounded_send(p).unwrap();
            } else {
                // got ACK
                p.slot = -p.slot;
                assert!(p.slot <= self.slots_generation.len() as isize);
                if self.slots_generation[p.slot as usize - 1].load(Ordering::Acquire)
                    == p.generation
                {
                    if self.slots_used[p.slot as usize - 1].compare_and_swap(
                        true,
                        false,
                        Ordering::Release,
                    ) {
                        // only notify when it is originally used
                        self.notify.notify();
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn udp_loop<T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    drop_percentage: u64,
    from_fg: UnboundedReceiver<PacketType<T>>,
    to_fg: UnboundedSender<DataPacket<T>>,
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

pub fn start_udp_loop<T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static>(
    socket: UdpSocket,
    timeout: Duration,
    slot_capacity: usize,
    drop_percentage: u64
) -> (UnboundedSender<PacketType<T>>, UnboundedReceiver<DataPacket<T>>) {
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    thread::spawn(move || {
        udp_loop::<T>(socket, timeout, slot_capacity, drop_percentage, from_foreground, to_foreground);
    });
    (to_background, from_background)
}
