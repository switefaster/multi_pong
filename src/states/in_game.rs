use amethyst::{
    assets::{AssetStorage, Loader, Handle},
    core::transform::Transform,
    prelude::*,
    renderer::{Camera, ImageFormat, SpriteRender, SpriteSheet, SpriteSheetFormat, Texture},
    ui::{Anchor, TtfFormat, UiText, UiTransform},
};
use crate::states::{CurrentState, PlayerNameResource};
use crate::constants::{SCENE_WIDTH, SCENE_HEIGHT, PADDLE_WIDTH};
use crate::systems::{Paddle, Role};

pub struct InGame;

impl SimpleState for InGame {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let sprite_sheet = load_sprite_sheet(data.world);

        setup_camera(data.world);
        setup_paddles(data.world, sprite_sheet);
        setup_name_tag(data.world);
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

fn setup_paddles(world: &mut World, sprite_sheet_handle: Handle<SpriteSheet>) {
    let mut own_transform = Transform::default();
    let mut hostile_transform = Transform::default();

    let y = SCENE_HEIGHT * 0.5;
    own_transform.set_translation_xyz(PADDLE_WIDTH * 0.5, y, 0.0);
    hostile_transform.set_translation_xyz(SCENE_WIDTH - PADDLE_WIDTH * 0.5, y, 0.0);

    let sprite_render = SpriteRender {
        sprite_sheet: sprite_sheet_handle,
        sprite_number: 0,
    };

    world
        .create_entity()
        .with(Paddle::new(Role::Own))
        .with(own_transform)
        .with(sprite_render.clone())
        .build();

    world
        .create_entity()
        .with(Paddle::new(Role::Hostile))
        .with(hostile_transform)
        .with(sprite_render)
        .build();
}

fn setup_ball() {

}

fn setup_score() {

}

fn setup_name_tag(world: &mut World) {
    let name_resource = (*world.read_resource::<PlayerNameResource>()).clone();
    let font = world.read_resource::<Loader>().load(
        "font/square.ttf",
        TtfFormat,
        (),
        &world.read_resource(),
    );
    let my_transform = UiTransform::new(
        "my_name".to_string(),
        Anchor::TopMiddle,
        Anchor::Middle,
        -180.0,
        -10.0,
        1.0,
        150.0,
        20.0,
    );
    let hostile_transform = UiTransform::new(
        "hostile_name".to_string(),
        Anchor::TopMiddle,
        Anchor::Middle,
        180.0,
        -10.0,
        1.0,
        150.0,
        20.0,
    );

    world
        .create_entity()
        .with(my_transform)
        .with(UiText::new(
            font.clone(),
            name_resource.my_name.unwrap(),
            [1.0, 1.0, 1.0, 1.0],
            20.0,
        ))
        .build();

    world
        .create_entity()
        .with(hostile_transform)
        .with(UiText::new(
            font,
            name_resource.rival_name.unwrap(),
            [1.0, 1.0, 1.0, 1.0],
            20.0,
        ))
        .build();
}

fn load_sprite_sheet(world: &mut World) -> Handle<SpriteSheet> {
    let texture_handle = {
        let loader = world.read_resource::<Loader>();
        let texture_storage = world.read_resource::<AssetStorage<Texture>>();
        loader.load(
            "texture/pong_spritesheet.png",
            ImageFormat::default(),
            (),
            &texture_storage,
        )
    };
    let loader = world.read_resource::<Loader>();
    let sprite_sheet_store = world.read_resource::<AssetStorage<SpriteSheet>>();
    loader.load(
        "texture/pong_spritesheet.ron",
        SpriteSheetFormat(texture_handle),
        (),
        &sprite_sheet_store,
    )
}
