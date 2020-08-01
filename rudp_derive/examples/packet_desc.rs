use rudp_derive::PacketDesc;

#[derive(serde::Serialize, serde::Deserialize, PacketDesc)]
pub enum Packet {
    A,
    #[packet(reliable)]
    B,
    #[packet(ordered)]
    C,
    #[packet(reliable, ordered)]
    D,
    #[packet(reliable, unordered)]
    E,
}

fn main() {

}