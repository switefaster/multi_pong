use amethyst::{
    core::{
        shrev::EventChannel,
    },
    derive::SystemDesc,
    ecs::{System, SystemData, Write},
};
use crate::network::{ResponseState, NetworkCommunication};

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
