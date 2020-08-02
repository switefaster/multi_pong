use rudp_derive::PacketDesc;

#[derive(serde::Serialize, serde::Deserialize, PacketDesc, PartialEq, Debug)]
pub enum Packet {
    RandomPacket {
        number: u32,
    },
    #[packet(reliable)]
    Handshake {
        timestamp: u128,
    },
    #[packet(ordered)]
    PaddleMovement {
        position: f32,
    },
    #[packet(reliable, ordered)]
    ReliableOrderedPacket {
       number: u32,
    },
}

fn main() {
    use rudp::PacketDesc;
    let handshake = Packet::Handshake {
        timestamp: 0,
    };
    assert_eq!(handshake.reliable(), true);
    assert_eq!(Packet::ordered(handshake.id()), false);
    let mut writer = Vec::<u8>::new();
    handshake.serialize(&mut writer);
    assert_eq!(Packet::deserialize(handshake.id(), &writer).unwrap(), handshake);
}