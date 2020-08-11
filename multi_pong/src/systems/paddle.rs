use crate::constants::{DEFAULT_ROTATION, PADDLE_HEIGHT, PADDLE_WIDTH, SCENE_HEIGHT, SCENE_WIDTH};
use crate::network::Packet::PaddleDisplace;
use crate::network::{NetworkCommunication, Packet};
use amethyst::prelude::World;
use amethyst::window::ScreenDimensions;
use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        timing::Time,
        transform::Transform,
        SystemDesc,
    },
    ecs::{Component, DenseVecStorage, Join, Read, System, SystemData, WriteStorage},
    input::{InputHandler, StringBindings},
};

#[derive(Default)]
pub struct PaddleSystemDesc;

impl<'a, 'b> SystemDesc<'a, 'b, PaddleSystem> for PaddleSystemDesc {
    fn build(self, world: &mut World) -> PaddleSystem {
        <PaddleSystem as System<'_>>::SystemData::setup(world);
        let reader = world.fetch_mut::<EventChannel<Packet>>().register_reader();
        PaddleSystem::new(reader)
    }
}

pub struct PaddleSystem {
    reader: ReaderId<Packet>,
}

impl PaddleSystem {
    fn new(reader: ReaderId<Packet>) -> Self {
        Self { reader }
    }
}

impl<'a> System<'a> for PaddleSystem {
    type SystemData = (
        Read<'a, Time>,
        Read<'a, NetworkCommunication>,
        Read<'a, InputHandler<StringBindings>>,
        Read<'a, EventChannel<Packet>>,
        Read<'a, ScreenDimensions>,
        WriteStorage<'a, Transform>,
        WriteStorage<'a, Paddle>,
    );

    fn run(
        &mut self,
        (time, comm, input, event_channel, dimension, mut transforms, mut paddles): Self::SystemData,
    ) {
        for (transform, paddle) in (&mut transforms, &mut paddles).join() {
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
                        let rotation = {
                            if let Some(mouse_pos) = input.mouse_position() {
                                let paddle_x =
                                    transform.translation().x * dimension.width() / SCENE_WIDTH;
                                let paddle_y = 500.0 - paddle_y * dimension.height() / SCENE_HEIGHT;
                                let dir_x = mouse_pos.0 - paddle_x;
                                let dir_y = mouse_pos.1 - paddle_y;
                                dir_y.atan2(dir_x)
                            } else {
                                0.0
                            }
                        };
                        transform.set_rotation_2d(-rotation);
                        paddle.rotation = -rotation;
                        if let Some(ref sender) = comm.sender {
                            sender
                                .unbounded_send(Packet::PaddleDisplace { position, rotation })
                                .unwrap();
                        }
                    }
                }
                Role::Hostile => {
                    for event in event_channel.read(&mut self.reader) {
                        if let PaddleDisplace { position, rotation } = event {
                            transform.set_translation_y(
                                position
                                    .min(SCENE_HEIGHT - PADDLE_HEIGHT * 0.5)
                                    .max(PADDLE_HEIGHT * 0.5),
                            );
                            transform.set_rotation_2d(*rotation);
                            paddle.rotation = *rotation;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum Role {
    Own,
    Hostile,
}

pub struct Paddle {
    pub role: Role,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
}

impl Paddle {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            width: PADDLE_WIDTH,
            height: PADDLE_HEIGHT,
            rotation: DEFAULT_ROTATION,
        }
    }
}

impl Component for Paddle {
    type Storage = DenseVecStorage<Self>;
}
