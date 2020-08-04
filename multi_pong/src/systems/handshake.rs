use crate::network::{NetworkCommunication, Packet};
use crate::states::{CurrentState, PlayerNameResource};
use amethyst::prelude::World;
use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        SystemDesc,
    },
    ecs::{Read, System, SystemData, Write},
};

#[derive(Default)]
pub struct HandshakeSystemDesc;

impl<'a, 'b> SystemDesc<'a, 'b, HandshakeSystem> for HandshakeSystemDesc {
    fn build(self, world: &mut World) -> HandshakeSystem {
        <HandshakeSystem as System<'_>>::SystemData::setup(world);
        let reader = world.fetch_mut::<EventChannel<Packet>>().register_reader();
        HandshakeSystem::new(reader)
    }
}

pub struct HandshakeSystem {
    reader: ReaderId<Packet>,
}

impl HandshakeSystem {
    fn new(reader: ReaderId<Packet>) -> Self {
        Self { reader }
    }
}

impl<'a> System<'a> for HandshakeSystem {
    type SystemData = (
        Read<'a, EventChannel<Packet>>,
        Read<'a, NetworkCommunication>,
        Write<'a, PlayerNameResource>,
        Write<'a, CurrentState>,
    );

    fn run(&mut self, (event_channel, comm, mut name, mut state): Self::SystemData) {
        for event in event_channel.read(&mut self.reader) {
            if let Packet::Handshake { player_name } = event {
                let is_server = comm.is_server();
                if let Some(ref sender) = comm.sender {
                    if is_server {
                        sender
                            .unbounded_send(Packet::Handshake {
                                player_name: name.my_name.clone().unwrap(),
                            })
                            .unwrap();
                        println!("Handshake packet responded!")
                    }
                    name.rival_name = Some(player_name.clone());
                    *state = CurrentState::InGame;
                }
            }
        }
    }
}
