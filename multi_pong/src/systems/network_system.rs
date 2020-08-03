use crate::network::{NetworkCommunication, Packet};
use amethyst::{
    core::shrev::EventChannel,
    derive::SystemDesc,
    ecs::{System, SystemData, Write},
};

#[derive(SystemDesc)]
pub struct NetworkBroadcastingSystem;

impl<'a> System<'a> for NetworkBroadcastingSystem {
    type SystemData = (
        Write<'a, EventChannel<Packet>>,
        Write<'a, NetworkCommunication>,
    );

    fn run(&mut self, (mut event_channel, mut network_communication): Self::SystemData) {
        if let Some(ref mut recv) = network_communication.receiver {
            while let Ok(Some(packet)) = recv.try_next() {
                event_channel.single_write(packet);
            }
        }
    }
}
