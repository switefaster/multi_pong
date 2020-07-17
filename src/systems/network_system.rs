use amethyst::{
    core::{
        shrev::EventChannel,
        SystemDesc,
    },
    derive::SystemDesc,
    ecs::{System, SystemData, Write},
};
use crate::network::{ResponseState, Instruction};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

#[derive(Default)]
pub struct NetworkCommunication {
    receiver: Option<UnboundedReceiver<ResponseState>>,
    sender: Option<UnboundedSender<Instruction>>,
}

impl NetworkCommunication {
    pub fn new(
        receiver: UnboundedReceiver<ResponseState>,
        sender: UnboundedSender<Instruction>,
    ) -> Self {
        Self {
            receiver: Some(receiver),
            sender: Some(sender),
        }
    }
}

#[derive(SystemDesc)]
pub struct NetworkBroadcastingSystem;

impl<'a> System<'a> for NetworkBroadcastingSystem {
    type SystemData = (
        Write<'a, EventChannel<ResponseState>>,
        Write<'a, NetworkCommunication>,
    );

    fn run(&mut self, (mut event_channel, mut network_communication): Self::SystemData) {
        if let Some(ref mut comm) = network_communication.receiver {
            while let Ok(Some(state)) = comm.try_next() {
                event_channel.single_write(state);
            }
        }
    }
}
