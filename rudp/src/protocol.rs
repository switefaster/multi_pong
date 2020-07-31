use std::convert::TryInto;
use std::mem::size_of;
use std::rc::Rc;

pub trait PacketDesc {
    fn id(&self) -> u32;
    fn data(&self) -> Rc<Vec<u8>>;
    fn reliable(&self) -> bool;
    fn ordered(id: u32) -> bool;
    fn deserialize(id: u32, data: &[u8]) -> Self;
}

pub struct Packet<'a> {
    pub id: u32,
    pub slot: isize,
    pub generation: i64,
    pub data: &'a [u8],
}

const SLOT_START: usize = size_of::<u32>();
const GENERATION_START: usize = SLOT_START + size_of::<isize>();
const GENERATION_END: usize = GENERATION_START + size_of::<i64>();

impl<'a> Packet<'a> {
    pub fn new(data: &'a [u8], id: u32, slot: isize, generation: i64) -> Self {
        Packet {
            data,
            id,
            slot,
            generation,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut result: Vec<u8> = Vec::with_capacity(
            self.data.len() + size_of::<u32>() + size_of::<isize>() + size_of::<i64>(),
        );
        result.extend(self.id.to_be_bytes().iter());
        result.extend(self.slot.to_be_bytes().iter());
        result.extend(self.generation.to_be_bytes().iter());
        result.extend(self.data.iter());
        result
    }

    pub fn deserialize(data: &'a [u8]) -> Self {
        let id = u32::from_be_bytes(data[..SLOT_START].try_into().unwrap());
        let slot = isize::from_be_bytes(data[SLOT_START..GENERATION_START].try_into().unwrap());
        let generation =
            i64::from_be_bytes(data[GENERATION_START..GENERATION_END].try_into().unwrap());
        let data = &data[GENERATION_END..];
        Packet {
            id,
            slot,
            generation,
            data,
        }
    }
}

pub fn modify_header(data: &mut [u8], slot: isize, generation: i64) {
    data[SLOT_START..GENERATION_START].copy_from_slice(&slot.to_be_bytes());
    data[GENERATION_START..GENERATION_END].copy_from_slice(&generation.to_be_bytes());
}
