use crate::constants::{
    BALL_ANGULAR_SPEED, BALL_RADIUS, BALL_VELOCITY_X, BALL_VELOCITY_Y, PADDLE_WIDTH, SCENE_HEIGHT,
    SCENE_WIDTH,
};
use crate::network::{NetworkCommunication, GLOBAL_PING};
use crate::states::{CurrentState, PlayerNameResource};
use crate::systems::{Ball, Paddle, Role};
use amethyst::{
    assets::{AssetStorage, Handle, Loader},
    core::{timing::Time, transform::Transform},
    ecs::Entity,
    prelude::*,
    renderer::{Camera, ImageFormat, SpriteRender, SpriteSheet, SpriteSheetFormat, Texture},
    ui::{Anchor, TtfFormat, UiText, UiTransform},
};

#[derive(Default)]
pub struct UpdatableUI {
    own_score: Option<Entity>,
    hostile_score: Option<Entity>,
    ping: Option<Entity>,
}

#[derive(Default)]
pub struct InGame {
    ball_spawn_timer: Option<f32>,
    sprite_sheet_handle: Option<Handle<SpriteSheet>>,
}

impl SimpleState for InGame {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        self.ball_spawn_timer.replace(5.0);
        self.sprite_sheet_handle
            .replace(load_sprite_sheet(data.world));

        setup_camera(data.world);
        setup_paddles(data.world, self.sprite_sheet_handle.clone().unwrap());
        setup_name_tag(data.world);
    }

    fn on_resume(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        *data.world.write_resource::<CurrentState>() = CurrentState::InGame;
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        if let Some(mut timer) = self.ball_spawn_timer.take() {
            {
                let time = data.world.fetch::<Time>();
                timer -= time.delta_seconds();
            }
            if timer <= 0.0 {
                setup_ball(data.world, self.sprite_sheet_handle.clone().unwrap());
            } else {
                self.ball_spawn_timer.replace(timer);
            }
        }
        let ui = data.world.read_resource::<UpdatableUI>();
        if let Some(ping) = ui.ping {
            std::mem::drop(ui);
            let mut storage = data.world.write_storage::<UiText>();
            if let Some(text) = storage.get_mut(ping) {
                // try_lock() because we don't want the game thread get blocked
                if let Ok(ping) = GLOBAL_PING.try_lock() {
                    text.text = format!("{} ms", *ping);
                }
            }
        }
        Trans::None
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

fn setup_ball(world: &mut World, sprite_sheet_handle: Handle<SpriteSheet>) {
    let mut local_transform = Transform::default();
    local_transform.set_translation_xyz(SCENE_WIDTH * 0.5, SCENE_HEIGHT * 0.5, 0.0);

    let sprite_render = SpriteRender {
        sprite_sheet: sprite_sheet_handle,
        sprite_number: 1,
    };

    let comm = world.read_resource::<NetworkCommunication>();
    let is_server = comm.is_server();
    std::mem::drop(comm);

    if is_server {
        world
            .create_entity()
            .with(local_transform)
            .with(sprite_render)
            .with(Ball {
                velocity: [BALL_VELOCITY_X, BALL_VELOCITY_Y],
                radius: BALL_RADIUS,
                omega: BALL_ANGULAR_SPEED,
            })
            .build();
    } else if !is_server {
        world
            .create_entity()
            .with(local_transform)
            .with(sprite_render)
            .with(Ball {
                velocity: [-BALL_VELOCITY_X, BALL_VELOCITY_Y],
                radius: BALL_RADIUS,
                omega: BALL_ANGULAR_SPEED,
            })
            .build();
    }
}

fn setup_score() {}

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
    let ping_transform = UiTransform::new(
        "ping".to_string(),
        Anchor::BottomRight,
        Anchor::BottomRight,
        0.0,
        0.0,
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
            font.clone(),
            name_resource.rival_name.unwrap(),
            [1.0, 1.0, 1.0, 1.0],
            20.0,
        ))
        .build();

    let ping = world
        .create_entity()
        .with(ping_transform)
        .with(UiText::new(
            font,
            "0ms".to_string(),
            [1.0, 1.0, 1.0, 1.0],
            20.0,
        ))
        .build();

    world.insert(UpdatableUI {
        own_score: None,
        hostile_score: None,
        ping: Some(ping),
    });
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
