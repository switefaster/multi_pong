use crate::constants::{SCENE_HEIGHT, SCENE_WIDTH};
use crate::network::{NetworkCommunication, Packet};
use crate::systems::{Paddle, Role};
use amethyst::core::ecs::{ReadStorage, World};
use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        timing::Time,
        transform::Transform,
        SystemDesc,
    },
    derive::SystemDesc,
    ecs::{Component, DenseVecStorage, Join, Read, System, SystemData, Write, WriteStorage},
};

pub struct Ball {
    pub velocity: [f32; 2],
    pub radius: f32,
}

impl Component for Ball {
    type Storage = DenseVecStorage<Self>;
}

#[derive(Default)]
pub struct SyncBallSystemDesc;

impl<'a, 'b> SystemDesc<'a, 'b, SyncBallSystem> for SyncBallSystemDesc {
    fn build(self, world: &mut World) -> SyncBallSystem {
        <SyncBallSystem as System<'_>>::SystemData::setup(world);
        let reader = world.fetch_mut::<EventChannel<Packet>>().register_reader();
        SyncBallSystem::new(reader)
    }
}

pub struct SyncBallSystem {
    reader: ReaderId<Packet>,
    timer: f32,
}

impl SyncBallSystem {
    fn new(reader: ReaderId<Packet>) -> Self {
        Self {
            reader,
            timer: 0.02,
        }
    }
}

impl<'a> System<'a> for SyncBallSystem {
    type SystemData = (
        Read<'a, Time>,
        Write<'a, NetworkCommunication>,
        Read<'a, EventChannel<Packet>>,
        WriteStorage<'a, Transform>,
        WriteStorage<'a, Ball>,
    );

    fn run(
        &mut self,
        (time, mut comm, event_channel, mut transforms, mut balls): Self::SystemData,
    ) {
        self.timer -= time.delta_seconds();
        for (transform, ball) in (&mut transforms, &mut balls).join() {
            if comm.is_server() {
                if self.timer <= 0.0 {
                    if let Some(ref mut sender) = comm.sender {
                        sender
                            .unbounded_send(Packet::BallPosVel {
                                position: [transform.translation().x, transform.translation().y],
                                velocity: ball.velocity,
                            })
                            .unwrap();
                    }
                }
            }
            if comm.is_client() {
                for event in event_channel.read(&mut self.reader) {
                    if let Packet::BallPosVel { position, velocity } = event {
                        transform.set_translation_xyz(SCENE_WIDTH - position[0], position[1], 0.0);
                        ball.velocity[0] = -velocity[0];
                        ball.velocity[1] = velocity[1];
                    }
                }
            }
        }
        if self.timer <= 0.0 {
            self.timer = 0.02;
        }
    }
}

#[derive(SystemDesc)]
pub struct MoveBallSystem;

impl<'a> System<'a> for MoveBallSystem {
    type SystemData = (
        ReadStorage<'a, Ball>,
        WriteStorage<'a, Transform>,
        Read<'a, Time>,
    );

    fn run(&mut self, (balls, mut transforms, time): Self::SystemData) {
        for (ball, transforms) in (&balls, &mut transforms).join() {
            transforms.prepend_translation_x(ball.velocity[0] * time.delta_seconds());
            transforms.prepend_translation_y(ball.velocity[1] * time.delta_seconds());
        }
    }
}

#[derive(SystemDesc)]
pub struct BounceSystem;

impl<'a> System<'a> for BounceSystem {
    type SystemData = (
        WriteStorage<'a, Ball>,
        ReadStorage<'a, Paddle>,
        ReadStorage<'a, Transform>,
    );

    fn run(&mut self, (mut balls, paddles, transforms): Self::SystemData) {
        for (ball, transform) in (&mut balls, &transforms).join() {
            let ball_x = transform.translation().x;
            let ball_y = transform.translation().y;

            if (ball_y <= ball.radius && ball.velocity[1] < 0.0)
                || (ball_y >= SCENE_HEIGHT - ball.radius && ball.velocity[1] > 0.0)
            {
                ball.velocity[1] = -ball.velocity[1];
            }

            for (paddle, paddle_transform) in (&paddles, &transforms).join() {
                let paddle_x = paddle_transform.translation().x - (paddle.width * 0.5);
                let paddle_y = paddle_transform.translation().y - (paddle.height * 0.5);

                if point_in_rect(
                    ball_x,
                    ball_y,
                    paddle_x - ball.radius,
                    paddle_y - ball.radius,
                    paddle_x + paddle.width + ball.radius,
                    paddle_y + paddle.height + ball.radius,
                ) {
                    if (paddle.role == Role::Own && ball.velocity[0] < 0.0)
                        || (paddle.role == Role::Hostile && ball.velocity[0] > 0.0)
                    {
                        ball.velocity[0] = -ball.velocity[0];
                    }
                }
            }
        }
    }
}

fn point_in_rect(x: f32, y: f32, left: f32, bottom: f32, right: f32, top: f32) -> bool {
    x >= left && x <= right && y >= bottom && y <= top
}
