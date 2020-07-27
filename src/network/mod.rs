use futures::channel::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use std::thread;
use futures::{
    select_biased,
    future::FutureExt,
    pin_mut,
    sink::SinkExt
};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::stream::StreamExt;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec, FramedRead};
use tokio_serde::formats::SymmetricalMessagePack;
use async_std::sync::TrySendError;

pub enum Side {
    Server,
    Client,
    //This type exists only for Default implementation
    //Because we don't want the system to run either Server or Client logic by default
    //even if it's logically impossible that the system starts running before we insert
    //NetworkCommunication as Resource which we guarantee that it will only be created
    //with either Server or Client
    Unsure,
}

impl Default for Side {
    fn default() -> Self {
        Self::Unsure
    }
}

#[derive(Default)]
pub struct NetworkCommunication {
    pub(crate) receiver: Option<UnboundedReceiver<ResponseState>>,
    pub(crate) sender: Option<UnboundedSender<Instruction>>,
    side: Side,
}

impl NetworkCommunication {
    pub fn new(
        receiver: UnboundedReceiver<ResponseState>,
        sender: UnboundedSender<Instruction>,
        side: Side,
    ) -> Self {
        Self {
            receiver: Some(receiver),
            sender: Some(sender),
            side,
        }
    }

    pub fn is_client(&self) -> bool {
        if let Side::Client = self.side {
            true
        } else {
            false
        }
    }

    pub fn is_server(&self) -> bool {
        !self.is_client()
    }
}

pub enum DisconnectAction {
    End,
    WaitNew,
}

pub enum Instruction {
    SendPacket(Packet),
    Disconnect(DisconnectAction),
}

pub enum ResponseState {
    ConnectionAbort,
    PacketReceived(Packet),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Packet {
    Handshake {
        player_name: String,
    },
    PaddleDisplace {
        position: f32,
    },
}

pub fn create_server_background_loop() -> NetworkCommunication {
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    thread::spawn(move || {
        server_network_loop(from_foreground, to_foreground);
    });
    NetworkCommunication::new(
        from_background,
        to_background,
        Side::Server,
    )
}

pub fn create_client_background_loop<A: 'static + ToSocketAddrs + Send + Sync>(addr: A) -> NetworkCommunication {
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    thread::spawn(move || {
        client_network_loop(addr, from_foreground, to_foreground);
    });
    NetworkCommunication::new(
        from_background,
        to_background,
        Side::Client,
    )
}

#[tokio::main]
async fn client_network_loop<A: ToSocketAddrs>(addr: A, mut from_foreground: UnboundedReceiver<Instruction>, mut to_foreground: UnboundedSender<ResponseState>) {
    let mut socket = TcpStream::connect(addr).await.unwrap();
    socket.set_nodelay(true).unwrap();
    let client = async move {
        let (reader, writer) = socket.split();
        let length_delimited_write =
            FramedWrite::new(writer, LengthDelimitedCodec::new());
        let mut serialized =
            tokio_serde::SymmetricallyFramed::new(
                length_delimited_write,
                SymmetricalMessagePack::<Packet>::default(),
            );
        let length_delimited_read =
            FramedRead::new(reader, LengthDelimitedCodec::new());
        let mut deserialized =
            tokio_serde::SymmetricallyFramed::new(
                length_delimited_read,
                SymmetricalMessagePack::<Packet>::default(),
            );
        loop {
            let fg_to = from_foreground.next().fuse();
            let to_fg = deserialized.next().fuse();
            pin_mut!(fg_to, to_fg);
            select_biased! {
                inst = fg_to => {
                    if let Some(inst) = inst {
                        match inst {
                            Instruction::Disconnect(_) => break,
                            Instruction::SendPacket(packet) => {
                                serialized.send(packet).await.unwrap();
                            },
                        }
                    }
                },
                msg = to_fg => {
                    if let Some(msg) = msg {
                        to_foreground.send(ResponseState::PacketReceived(msg.unwrap())).await.unwrap();
                    }
                },
            };
        }
    };
    client.await
}

#[tokio::main]
async fn server_network_loop(mut from_foreground: UnboundedReceiver<Instruction>, mut to_foreground: UnboundedSender<ResponseState>) {
    let mut listener = TcpListener::bind("0.0.0.0:4001").await.unwrap();
    let (tx, rx) = async_std::sync::channel(1);
    let server = async move {
        let mut incoming = listener.incoming();
        while let Some(conn) = incoming.next().await {
            match conn {
                Err(e) => println!("connection failed: {:?}", e),
                Ok(sock) => {
                    if let Err(e) = tx.try_send(sock) {
                        match e {
                            TrySendError::Full(_sock) => {
                                //notify the connection
                            },
                            TrySendError::Disconnected(_sock) => {
                                //notify the connection
                                break;
                            }
                        }
                    }
                }
            }
        }
    };
    tokio::spawn(async move {
        'main: loop {
            let mut socket = rx.recv().await.unwrap();
            socket.set_nodelay(true).unwrap();
            let (reader, writer) = socket.split();
            let length_delimited_write =
                FramedWrite::new(writer, LengthDelimitedCodec::new());
            let mut serialized =
                tokio_serde::SymmetricallyFramed::new(
                    length_delimited_write,
                    SymmetricalMessagePack::<Packet>::default(),
                );
            let length_delimited_read =
                FramedRead::new(reader, LengthDelimitedCodec::new());
            let mut deserialized =
                tokio_serde::SymmetricallyFramed::new(
                    length_delimited_read,
                    SymmetricalMessagePack::<Packet>::default(),
                );
            loop {
                let fg_to = from_foreground.next().fuse();
                let to_fg = deserialized.next().fuse();
                pin_mut!(fg_to, to_fg);
                select_biased! {
                    inst = fg_to => {
                        if let Some(inst) = inst {
                            match inst {
                                Instruction::Disconnect(DisconnectAction::End) => break 'main,
                                Instruction::Disconnect(DisconnectAction::WaitNew) => continue 'main,
                                Instruction::SendPacket(packet) => {
                                    serialized.send(packet).await.unwrap();
                                },
                            }
                        }
                    },
                    msg = to_fg => {
                        if let Some(msg) = msg {
                            to_foreground.send(ResponseState::PacketReceived(msg.unwrap())).await.unwrap();
                        }
                    },
                };
            }
        }
    });
    server.await
}
