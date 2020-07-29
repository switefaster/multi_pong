use amethyst::{SimpleState, GameData, StateData, SimpleTrans, Trans};
use amethyst::ui::UiCreator;
use crate::network::create_server_background_loop;
use amethyst::prelude::WorldExt;
use crate::states::CurrentState;

pub struct ServerWait;

impl SimpleState for ServerWait {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/server_wait.ron", ());
        });
        world.insert(create_server_background_loop());
    }

    fn on_pause(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        data.world.delete_all();
    }

    fn on_resume(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        *data.world.write_resource::<CurrentState>() = CurrentState::Ui;
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        let state = data.world.read_resource::<CurrentState>();
        if let CurrentState::InGame = *state {
            Trans::Push(Box::new(super::InGame::default()))
        } else {
            Trans::None
        }
    }
}
