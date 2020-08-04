# RUDP

Roughly UDP, a UDP wrapper layer with optional reliable ordered (but not
sequenced) message transmission.

## Example
There is a `udp_remote` example in the examples directory.
```
# Setup a server listening on "0.0.0.0:4001"
cargo run --release --example udp_remote "0.0.0.0:4001"

# Setup a client listening on "0.0.0.0:4002" and connect to "0.0.0.0:4001"
cargo run --release --example udp_remote "0.0.0.0:4002" "0.0.0.0:4001"
```

## Features
* Handle network handshake between server and client, via magic byte string.
* Provide unreliable packet transmission, with optional order requirement.
* Provide reliable packet transmission.
* Provide unbounded channels (non-blocking send/receive) for use in the game loop.

* Unreliable means that the packet would only be sent once, just simple UDP.
* Ordered means that old packets with the same ID would be discarded, if
  reordered due to the network. This does *not* mean that we would receive every
  packet in a sequence.
* Reliable means that the system would try to resend the packet if no ACK is
  received after timeout. The packet is guaranteed to be received once and only
  once in the remote, if the remote can accept any packet. The packets may be
  sent in *any order*.

## Non-Goal
This is just an experiment, we would *not*:
* Handle all errors gracefully, as that is painful.
* Handle packet fragmentation, as we don't need that, our packets are quite
  small.
* Do congestion control, as I don't know how to do that, and not needed in our
  case.

## Protocol
> Copied from the tracking issue above.

Reliable transmission is done by adding an acknowledgment (ACK) message into our
system. If the sender did not receive ACK after the message timeout, it would
resend the message. In order to avoid ACK being lost which may cause the
receiver to receive the same message two times, we would also track the received
packet in the receiver side to discard any duplicated message from the sender.
This is implemented in both the sender and receiver by slot index and generation
ID. We have N slots in both the sender and receiver side, each reliable message
would be put into a slot, and it would get the slot ID and generation ID.
Generation ID would only increment for different messages in the same slot. (it
could wrap around, so we should do wrapped arithmetic)

When the sender sends a reliable message, it would first wait for an empty slot,
store it into the slot and attach the slot ID and generation ID, and at last
send it through UDP. If it receives the ACK from the receiver, it would first
check if the ACK generation matches the stored packet, and empty the slot if
yes.

When the receiver receives the message, it would first send the ACK. Afterward,
it would check if the generation ID for that slot, and discard the message if
the received one is not newer (received - stored > 0). If not, it would forward
the packet to the application, and store the generation ID into that slot.

Currently, we implement the slot ID as isize, positive means message slot,
negative means ACK slot, 0 means no ACK needed.

Packet Priority:
1. ACK
2. Unreliable packet.
3. Reliable packet timeout retransmission.
4. Reliable packet transmission.

