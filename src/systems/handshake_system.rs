use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        SystemDesc,
    },
    derive::SystemDesc,
    ecs::{Read, System, SystemData, Write},
};
use crate::network::{Packet, Instruction, NetworkCommunication, ResponseState};
use futures::SinkExt;
use crate::states::{PlayerNameResource, CurrentState};
use amethyst::prelude::World;

#[derive(Default)]
pub struct HandshakeSystemDesc;

impl<'a, 'b> SystemDesc<'a, 'b, HandshakeSystem> for HandshakeSystemDesc {
    fn build(self, world: &mut World) -> HandshakeSystem {
        <HandshakeSystem as System<'_>>::SystemData::setup(world);
        let reader = world
            .fetch_mut::<EventChannel<ResponseState>>()
            .register_reader();
        HandshakeSystem::new(reader)
    }
}

pub struct HandshakeSystem {
    reader: ReaderId<ResponseState>,
}

impl HandshakeSystem {
    fn new(reader: ReaderId<ResponseState>) -> Self {
        Self {
            reader,
        }
    }
}

impl<'a> System<'a> for HandshakeSystem {
    type SystemData = (
        Read<'a, EventChannel<ResponseState>>,
        Write<'a, NetworkCommunication>,
        Write<'a, PlayerNameResource>,
        Write<'a, CurrentState>,
    );

    fn run(&mut self, (event_channel, mut comm, mut name, mut state): Self::SystemData) {
        for event in event_channel.read(&mut self.reader) {
            if let ResponseState::PacketReceived(Packet::Handshake {player_name}) = event {
                let is_server = comm.is_server();
                if let Some(ref mut sender) = comm.sender {
                    if is_server {
                        sender.send(Instruction::SendPacket(Packet::Handshake {
                            player_name: name.my_name.clone().unwrap(),
                        }));
                    }
                    name.rival_name = Some(player_name.clone());
                    *state = CurrentState::InGame;
                }
            }
        }
    }
}
