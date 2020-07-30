mod winner;
mod ball;
mod paddle;
mod handshake;
mod network_system;

pub use paddle::{Paddle, Role};
pub use ball::Ball;

pub struct MultiPongBundle;

use amethyst::core::SystemBundle;
use amethyst::ecs::DispatcherBuilder;
use amethyst::Error;
use amethyst::prelude::{World, SystemDesc};
use crate::systems::network_system::NetworkBroadcastingSystem;
use crate::systems::handshake::HandshakeSystemDesc;
use crate::systems::paddle::PaddleSystemDesc;
use crate::systems::ball::{SyncBallSystemDesc, MoveBallSystem, BounceSystem};
use crate::systems::winner::WinnerSystem;

impl<'a, 'b> SystemBundle<'a, 'b> for MultiPongBundle {
    fn build(self, world: &mut World, dispatcher: &mut DispatcherBuilder<'a, 'b>) -> Result<(), Error> {
        dispatcher.add(
            NetworkBroadcastingSystem,
            "network_broadcasting_system",
            &[],
        );
        dispatcher.add(
            HandshakeSystemDesc::default().build(world),
            "handshake_system",
            &[],
        );
        dispatcher.add(
            PaddleSystemDesc::default().build(world),
            "paddle_system",
            &["input_system"],
        );
        dispatcher.add(
            SyncBallSystemDesc::default().build(world),
            "sync_ball_system",
            &[],
        );
        dispatcher.add(
            MoveBallSystem,
            "move_ball_system",
            &["sync_ball_system"],
        );
        dispatcher.add(
            BounceSystem,
            "collision_system",
            &["paddle_system", "move_ball_system"],
        );
        dispatcher.add(
            WinnerSystem,
            "winner_system",
            &["move_ball_system"],
        );
        Ok(())
    }
}
