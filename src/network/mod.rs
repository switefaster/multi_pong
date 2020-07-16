use std::sync::mpsc::{SyncSender, Receiver, sync_channel};
use std::thread;
use std::net::TcpStream;
use std::io::Write;

pub const MAX_PACKET: usize = 200;

pub enum DisconnectAction {
    End,
    WaitNew,
}

pub enum Instruction {
    SendPacket(Packet),
    Disconnect(DisconnectAction),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Packet {

}

pub fn create_background_loop() -> (SyncSender<Instruction>, Receiver<Result<Packet, std::io::Error>>) {
    let (to_background, from_foreground) = sync_channel(MAX_PACKET);
    let (to_foreground, from_background) = sync_channel(MAX_PACKET);
    thread::spawn(move || {
        match std::net::TcpListener::bind("0.0.0.0:4001") {
            Ok(listener) => {
                if let Err(error) = listener.set_nonblocking(true) {
                    to_foreground.send(Err(error)).unwrap();
                }
                let mut current_connection: Option<TcpStream> = None;
                let read_buffer = ringbuf::RingBuffer::<u8>::new(2048);
                'main: loop {
                    if let None = current_connection {
                        for connection in listener.incoming() {
                            if let Ok(connection) = connection {
                                current_connection = Some(connection);
                                break;
                            }
                        }
                    }
                    if let Some(mut connection) = current_connection.take() {
                        while let Ok(inst) = from_foreground.try_recv() {
                            match inst {
                                Instruction::Disconnect(action) => {
                                    match action {
                                        DisconnectAction::WaitNew => continue 'main,
                                        DisconnectAction::End => break 'main,
                                    }
                                },
                                Instruction::SendPacket(packet) => {
                                    let data = serialize_with_header(packet);
                                    if let Err(error) = connection.write(data.as_slice()) {
                                        to_foreground.send(Err(error)).unwrap();
                                    }
                                    if let Err(error) = connection.flush() {
                                        to_foreground.send(Err(error)).unwrap();
                                    }
                                },
                            }
                        }
                        let (mut prod, mut cons) = read_buffer.split();
                        prod.read_from(&mut connection, None);
                        loop {
                            if cons.remaining() < 8 {
                                break;
                            }
                            let mut header: usize = 0;
                            cons.access(|(_, new)| {
                                for i in 0..7 {
                                    header |= (new[i] as usize) << (i << 3);
                                }
                            });
                            if cons.remaining() < header + 8 {
                                break;
                            }
                            let _ = cons.read_bytes(8);
                            let data = cons.read_bytes(header).unwrap();
                            let packet: Packet = rmp_serde::from_read(data.as_slice()).unwrap();
                            to_foreground.send(Ok(packet));
                        }
                        current_connection = Some(connection);
                    }
                }
            },
            Err(error) => {
                to_foreground.send(Err(error)).unwrap();
            },
        }
    });
    (to_background, from_background)
}

fn serialize_with_header(packet: Packet) -> Vec<u8> {
    let mut data = rmp_serde::to_vec(&packet).unwrap();
    let len = data.len();
    let header: [u8; 8] = Default::default();
    let mut byte = 8u32;
    while byte >= 0 {
        header[8u32 - byte] = len & ((0xFF) << (byte - 1));
        byte -= 1;
    }
    data.splice(..0, header);
    data
}
