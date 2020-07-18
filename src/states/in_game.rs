use amethyst::{
    assets::{AssetStorage, Loader, Handle},
    core::transform::Transform,
    ecs::prelude::{Component, DenseVecStorage},
    prelude::*,
    renderer::{Camera, ImageFormat, SpriteRender, SpriteSheet, SpriteSheetFormat, Texture},
};
use crate::states::CurrentState;
use crate::constants::{SCENE_WIDTH, SCENE_HEIGHT};

pub struct InGame;

impl SimpleState for InGame {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        setup_camera(data.world);
    }

    fn on_resume(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        *data.world.write_resource::<CurrentState>() = CurrentState::InGame;
    }
}

fn setup_camera(world: &mut World) {
    let mut transform = Transform::default();
    transform.set_translation_xyz(SCENE_WIDTH * 0.5, SCENE_HEIGHT * 0.5, 1.0);

    world
        .create_entity()
        .with(Camera::standard_2d(SCENE_WIDTH, SCENE_HEIGHT))
        .with(transform)
        .build();
}

fn setup_paddles() {

}

fn setup_ball() {

}

fn setup_score() {

}
