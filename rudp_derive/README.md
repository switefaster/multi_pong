# RUDP Derive

Provide derive macro for `PacketDesc` trait in [RUDP](../rudp/README.md).

## Usage
```rust
//Note that `PacketDesc` will generate PacketDesc::data() and PacketDesc::deserialize()
//using serde_cbor by default.
#[derive(PacketDesc, serde::Serialize, serde::Deserialize)]
pub enum Packet {
    //If not attributed, the default is (unreliable, unordered)
    #[packet(reliable, ordered)]
    ReliableOrdered,
    #[packet(unreliable, unordered)]
    UnreliableUnordered,
}
```
