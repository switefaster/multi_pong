use crate::constants::SCENE_WIDTH;
use crate::network::NetworkCommunication;
use crate::systems::Ball;
use amethyst::{
    core::transform::Transform,
    derive::SystemDesc,
    ecs::prelude::{Join, Read, System, SystemData, WriteStorage},
};

#[derive(SystemDesc)]
pub struct WinnerSystem;

impl<'s> System<'s> for WinnerSystem {
    type SystemData = (
        WriteStorage<'s, Ball>,
        WriteStorage<'s, Transform>,
        Read<'s, NetworkCommunication>,
    );

    fn run(&mut self, (mut balls, mut locals, comm): Self::SystemData) {
        if comm.is_server() {
            for (ball, transform) in (&mut balls, &mut locals).join() {
                let ball_x = transform.translation().x;

                let did_hit = if ball_x <= -ball.radius {
                    true
                } else if ball_x >= SCENE_WIDTH + ball.radius {
                    true
                } else {
                    false
                };

                if did_hit {
                    ball.velocity[0] = -ball.velocity[0];
                    transform.set_translation_x(SCENE_WIDTH * 0.5);
                }
            }
        }
    }
}
