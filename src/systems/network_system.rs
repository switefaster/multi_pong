use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        SystemDesc,
    },
    derive::SystemDesc,
    ecs::{DispatcherBuilder, Read, System, SystemData, World, Write},
    prelude::*,
};
use crate::network::{ResponseState, Instruction};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

pub struct NetworkCommunication {
    receiver: UnboundedReceiver<ResponseState>,
    sender: UnboundedSender<Instruction>,
}

impl NetworkCommunication {
    pub fn new(
        receiver: UnboundedReceiver<ResponseState>,
        sender: UnboundedSender<Instruction>,
    ) -> Self {
        Self {
            receiver,
            sender,
        }
    }
}

#[derive(SystemDesc)]
pub struct NetworkBroadcastingSystem;

impl<'a> System<'a> for NetworkBroadcastingSystem {
    type SystemData = (
        Write<'a, EventChannel<ResponseState>>,
        Read<'a, NetworkCommunication>,
    );

    fn run(&mut self, (mut event_channel, network_communication): Self::SystemData) {
        while let Ok(Some(state)) = network_communication.receiver.try_next() {
            event_channel.single_write(state);
        }
    }
}
