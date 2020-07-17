use futures::channel::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use std::thread;
use futures::{
    select_biased,
    future::FutureExt,
    pin_mut,
    sink::SinkExt
};
use tokio::net::TcpListener;
use tokio::stream::StreamExt;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec, FramedRead};
use tokio_serde::formats::SymmetricalMessagePack;
use async_std::sync::TrySendError;

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

}

pub fn create_background_loop() -> (UnboundedSender<Instruction>, UnboundedReceiver<ResponseState>) {
    let (to_background, from_foreground) = unbounded();
    let (to_foreground, from_background) = unbounded();
    thread::spawn(move || {
        network_loop(from_foreground, to_foreground);
    });
    (to_background, from_background)
}

#[tokio::main]
async fn network_loop(mut from_foreground: UnboundedReceiver<Instruction>, mut to_foreground: UnboundedSender<ResponseState>) {
    let mut listener = TcpListener::bind("0.0.0.0:4001").await.unwrap();
    let (tx, rx) = async_std::sync::channel(1);
    let server = async move {
        let mut incoming = listener.incoming();
        while let Some(conn) = incoming.next().await {
            match conn {
                Err(e) => println!("connection failed: {:?}", e),
                Ok(mut sock) => {
                    if let Err(e) = tx.try_send(sock) {
                        match e {
                            TrySendError::Full(_sock) => {
                                //notify the connection
                            },
                            TrySendError::Disconnected(_sock) => {
                                //notify the connection
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
                                Instruction::Disconnect(action) => {
                                    match action {
                                        DisconnectAction::End => break 'main,
                                        DisconnectAction::WaitNew => continue 'main,
                                    }
                                },
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
