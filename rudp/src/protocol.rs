use std::convert::TryInto;
use std::mem::size_of;

pub trait PacketDesc: Sized {
    /// Return the ID for the message, for checking message order.
    fn id(&self) -> u32;
    /// Serialize the message into the writer. Note that the ID is handled by the protocol header.
    /// The writer is non-empty, as it already conains the message header.
    fn serialize(&self, writer: &mut Vec<u8>);
    /// Return if the message is treated as reliable. Reliable packets would be resent if no ACK is
    /// received on time.
    fn reliable(&self) -> bool;
    /// Return if the message with particular ID would be treated as ordered. Old ordered messages
    /// would be discarded directly.
    fn ordered(id: u32) -> bool;
    /// Deserialize the data based on the ID and the remaining payload. Data should be the same as
    /// the data written into the writer in `serialize function`.
    fn deserialize(id: u32, data: &[u8]) -> Result<Self, DeserializeError>;
}

pub struct PacketHeader {
    pub id: u32,
    pub slot: isize,
    pub generation: i64,
}

pub struct DeserializeError(pub String);

const SLOT_START: usize = size_of::<u32>();
const GENERATION_START: usize = SLOT_START + size_of::<isize>();
const GENERATION_END: usize = GENERATION_START + size_of::<i64>();

impl PacketHeader {
    pub fn new(id: u32, slot: isize, generation: i64) -> Self {
        PacketHeader {
            id,
            slot,
            generation,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut result: Vec<u8> =
            Vec::with_capacity(size_of::<u32>() + size_of::<isize>() + size_of::<i64>());
        result.extend(self.id.to_be_bytes().iter());
        result.extend(self.slot.to_be_bytes().iter());
        result.extend(self.generation.to_be_bytes().iter());
        result
    }

    pub fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), DeserializeError> {
        if data.len() < GENERATION_END {
            return Err(DeserializeError(
                "Data shorter than header length.".to_string(),
            ));
        }
        let id = u32::from_be_bytes(
            data[..SLOT_START]
                .try_into()
                .map_err(|_| DeserializeError("Error deserializing ID.".to_string()))?,
        );
        let slot = isize::from_be_bytes(
            data[SLOT_START..GENERATION_START]
                .try_into()
                .map_err(|_| DeserializeError("Error deserializing slot ID.".to_string()))?,
        );
        let generation =
            i64::from_be_bytes(data[GENERATION_START..GENERATION_END].try_into().map_err(
                |_| DeserializeError("Error deserializing generation index.".to_string()),
            )?);
        let data = &data[GENERATION_END..];
        Ok((
            PacketHeader {
                id,
                slot,
                generation,
            },
            data,
        ))
    }
}

pub fn modify_header(data: &mut [u8], id: u32, slot: isize, generation: i64) {
    data[..SLOT_START].copy_from_slice(&id.to_be_bytes());
    data[SLOT_START..GENERATION_START].copy_from_slice(&slot.to_be_bytes());
    data[GENERATION_START..GENERATION_END].copy_from_slice(&generation.to_be_bytes());
}

pub fn modify_slot(data: &mut [u8], slot: isize, generation: i64) {
    data[SLOT_START..GENERATION_START].copy_from_slice(&slot.to_be_bytes());
    data[GENERATION_START..GENERATION_END].copy_from_slice(&generation.to_be_bytes());
}
