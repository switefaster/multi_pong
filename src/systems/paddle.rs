use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        SystemDesc,
        timing::Time,
        transform::Transform,
    },
    ecs::{Component, DenseVecStorage, Join, Read, ReadStorage, System, SystemData, Write, WriteStorage},
    input::{InputHandler, StringBindings},
};
use amethyst::prelude::World;
use crate::constants::{PADDLE_WIDTH, PADDLE_HEIGHT, SCENE_HEIGHT};
use crate::network::{ResponseState, NetworkCommunication, Instruction, Packet};
use crate::network::ResponseState::PacketReceived;
use crate::network::Packet::PaddleDisplace;

#[derive(Default)]
pub struct PaddleSystemDesc;

impl<'a, 'b> SystemDesc<'a, 'b, PaddleSystem> for PaddleSystemDesc {
    fn build(self, world: &mut World) -> PaddleSystem {
        <PaddleSystem as System<'_>>::SystemData::setup(world);
        let reader = world
            .fetch_mut::<EventChannel<ResponseState>>()
            .register_reader();
        PaddleSystem::new(reader)
    }
}

pub struct PaddleSystem {
    reader: ReaderId<ResponseState>,
}

impl PaddleSystem {
    fn new(reader: ReaderId<ResponseState>) -> Self {
        Self {
            reader,
        }
    }
}

impl<'a> System<'a> for PaddleSystem {
    type SystemData = (
        Read<'a, Time>,
        Write<'a, NetworkCommunication>,
        Read<'a, InputHandler<StringBindings>>,
        Read<'a, EventChannel<ResponseState>>,
        WriteStorage<'a, Transform>,
        ReadStorage<'a, Paddle>,
    );

    fn run(&mut self, (time, mut comm, input, event_channel, mut transforms, paddles): Self::SystemData) {
        for (transform, paddle) in (&mut transforms, &paddles).join() {
            match paddle.role {
                Role::Own => {
                    let movement = input.axis_value("paddle");
                    if let Some(mv_amount) = movement {
                        let scaled_amount = 36.0 * mv_amount as f32 * time.delta_seconds();
                        let paddle_y = transform.translation().y;
                        let position = (paddle_y + scaled_amount)
                            .min(SCENE_HEIGHT - PADDLE_HEIGHT * 0.5)
                            .max(PADDLE_HEIGHT * 0.5);
                        transform.set_translation_y(position);
                        if let Some(ref mut sender) = comm.sender {
                            sender.unbounded_send(Instruction::SendPacket(Packet::PaddleDisplace {
                                position,
                            })).unwrap();
                        }
                    }
                },
                Role::Hostile => {
                    for event in event_channel.read(&mut self.reader) {
                        if let PacketReceived(PaddleDisplace{ position }) = event {
                            transform.set_translation_y(
                                position
                                    .min(SCENE_HEIGHT - PADDLE_HEIGHT * 0.5)
                                    .max(PADDLE_HEIGHT * 0.5),
                            );
                        }
                    }
                },
            }
        }
    }
}

pub enum Role {
    Own,
    Hostile,
}

pub struct Paddle {
    pub role: Role,
    pub width: f32,
    pub height: f32,
}

impl Paddle {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            width: PADDLE_WIDTH,
            height: PADDLE_HEIGHT,
        }
    }
}

impl Component for Paddle {
    type Storage = DenseVecStorage<Self>;
}
