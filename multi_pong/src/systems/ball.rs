use crate::constants::{SCENE_HEIGHT, SCENE_WIDTH};
use crate::network::{NetworkCommunication, Packet};
use crate::systems::{Paddle, Role};
use tokio::time::Instant;
use amethyst::core::ecs::{ReadStorage, World};
use amethyst::{
    core::{
        shrev::{EventChannel, ReaderId},
        timing::Time,
        transform::Transform,
        SystemDesc,
    },
    derive::SystemDesc,
    ecs::{Component, DenseVecStorage, Join, Read, System, SystemData, WriteStorage},
};

pub struct Ball {
    pub velocity: [f32; 2],
    pub omega: f32,
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
        Read<'a, NetworkCommunication>,
        Read<'a, EventChannel<Packet>>,
        Read<'a, Option<Instant>>,
        WriteStorage<'a, Transform>,
        WriteStorage<'a, Ball>,
    );

    fn run(&mut self, (time, comm, event_channel, start_time, mut transforms, mut balls): Self::SystemData) {
        self.timer -= time.delta_seconds();
        for (transform, ball) in (&mut transforms, &mut balls).join() {
            // start_time should be present when this is run
            // TODO: interchange server and client role after collision with paddle,
            // and add generation for the messages to prevent confusion.
            let start_time = start_time.unwrap();
            if comm.is_server() {
                if self.timer <= 0.0 {
                    if let Some(ref sender) = comm.sender {
                        sender
                            .unbounded_send(Packet::BallPosVel {
                                generation: 0,
                                timestamp: start_time.elapsed().as_micros(),
                                position: [transform.translation().x, transform.translation().y],
                                velocity: ball.velocity,
                            })
                            .unwrap();
                    }
                }
            }
            if comm.is_client() {
                for event in event_channel.read(&mut self.reader) {
                    if let Packet::BallPosVel { timestamp, position, velocity, .. } = event {
                        let delta_s = (start_time.elapsed().as_micros() - timestamp) as f32 / 1_000_000.0;
                        // TODO: collision test here
                        transform.set_translation_xyz(
                            SCENE_WIDTH - position[0] - velocity[0] * delta_s,
                            position[1] + velocity[1] * delta_s,
                            0.0
                        );
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
            transforms.rotate_2d(ball.omega * time.delta_seconds());
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
                let paddle_x = paddle_transform.translation().x;
                let paddle_y = paddle_transform.translation().y;

                let paddle_dir = (paddle.rotation as f64).sin_cos();

                let paddle_rect = DirectionalRect {
                    pos: (paddle_x as f64, paddle_y as f64),
                    direction: (paddle_dir.1, paddle_dir.0),
                    extent: ((paddle.width * 0.5) as f64, (paddle.height * 0.5) as f64),
                };

                // maybe we should store it in Ball just as what Paddle does?
                let ball_dir = (transform.rotation().angle() as f64).sin_cos();

                let ball_rect = DirectionalRect {
                    pos: (ball_x as f64, ball_y as f64),
                    direction: (ball_dir.1, ball_dir.0),
                    extent: (ball.radius as f64, ball.radius as f64),
                };

                if let Some((vel_x, vel_y)) = paddle_rect.collide_and_reflect(
                    &ball_rect,
                    (ball.velocity[0] as f64, ball.velocity[1] as f64),
                ) {
                    if (paddle.role == Role::Own && vel_x > 0.0)
                        || (paddle.role == Role::Hostile && vel_x < 0.0)
                    {
                        ball.velocity[0] = vel_x as f32;
                        ball.velocity[1] = vel_y as f32;
                    }
                }
            }
        }
    }
}

struct DirectionalRect {
    pos: (f64, f64),
    // must normalized
    direction: (f64, f64),
    // extent is half of the width and height
    extent: (f64, f64),
}

impl DirectionalRect {
    fn collide_with(&self, another: &Self) -> bool {
        let pos_diff = (another.pos.0 - self.pos.0, another.pos.1 - self.pos.1);
        let vecs = [
            mul_complex((self.extent.0, 0.0), self.direction),
            mul_complex((0.0, self.extent.1), self.direction),
            mul_complex((another.extent.0, 0.0), another.direction),
            mul_complex((0.0, another.extent.1), another.direction),
        ];
        !vecs.iter().any(|vec| {
            let length = (vec.0 * vec.0 + vec.1 * vec.1).sqrt();
            let project = |v: (f64, f64)| (v.0 * vec.0 + v.1 * vec.1).abs() / length;
            let d = project(pos_diff)
                - project(vecs[0])
                - project(vecs[1])
                - project(vecs[2])
                - project(vecs[3]);
            d > 0.0
        })
    }

    fn reflect_to(&self, in_velocity: (f64, f64)) -> (f64, f64) {
        let twice_dot = 2.0 * (in_velocity.0 * self.direction.0 + in_velocity.1 * self.direction.1);
        (
            in_velocity.0 - self.direction.0 * twice_dot,
            in_velocity.1 - self.direction.1 * twice_dot,
        )
    }

    fn collide_and_reflect(&self, another: &Self, in_velocity: (f64, f64)) -> Option<(f64, f64)> {
        if self.collide_with(another) {
            Some(self.reflect_to(in_velocity))
        } else {
            None
        }
    }
}

fn mul_complex(src: (f64, f64), dst: (f64, f64)) -> (f64, f64) {
    (src.0 * dst.0 - src.1 * dst.1, src.1 * dst.0 + src.0 * dst.1)
}

#[cfg(test)]
mod test {
    use super::*;
    use float_cmp::*;

    fn tuple_approx_eq(lhs: (f64, f64), rhs: (f64, f64)) -> bool {
        // epsilon reached 10E-15 in test cases, however we just need 10E-6
        approx_eq!(f64, lhs.0, rhs.0, epsilon = 0.000001)
            && approx_eq!(f64, lhs.1, rhs.1, epsilon = 0.000001)
    }

    #[test]
    fn test_complex_mul() {
        let a = (1.0, 0.0);
        let b = (0.0, 1.0);
        assert!(tuple_approx_eq(mul_complex(a, b), (0.0, 1.0)));
        let a = (0.0, 1.0);
        let b = (-1.0, 0.0);
        assert!(tuple_approx_eq(mul_complex(a, b), (0.0, -1.0)));
        let a = ((0.5 as f64).sqrt(), (0.5 as f64).sqrt());
        assert!(tuple_approx_eq(mul_complex(a, a), (0.0, 1.0)));
    }

    #[test]
    fn test_collide_with() {
        let rect_a = DirectionalRect {
            pos: (0.0, 0.0),
            direction: ((0.5 as f64).sqrt(), (0.5 as f64).sqrt()),
            extent: (0.5, 0.5),
        };
        let rect_b = DirectionalRect {
            pos: (1.1, 0.0),
            direction: (1.0, 0.0),
            extent: (0.5, 0.5),
        };
        assert_eq!(rect_a.collide_with(&rect_b), true);
        assert_eq!(rect_b.collide_with(&rect_a), true);
        let rect_a = DirectionalRect {
            direction: (1.0, 0.0),
            ..rect_a
        };
        assert_eq!(rect_a.collide_with(&rect_b), false);
        assert_eq!(rect_b.collide_with(&rect_a), false);
    }

    #[test]
    fn test_reflect_to() {
        let reflect_in = (-2.0, 0.0);
        let rect_a = DirectionalRect {
            pos: (0.0, 0.0),
            direction: (1.0, 0.0),
            extent: (0.5, 0.5),
        };
        assert!(tuple_approx_eq(rect_a.reflect_to(reflect_in), (2.0, 0.0)));
        let rect_a = DirectionalRect {
            direction: ((0.5 as f64).sqrt(), (0.5 as f64).sqrt()),
            ..rect_a
        };
        assert!(tuple_approx_eq(rect_a.reflect_to(reflect_in), (0.0, 2.0)));
    }
}
