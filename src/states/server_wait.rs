use amethyst::{SimpleState, GameData, StateData, SimpleTrans, Trans};
use amethyst::ui::{UiCreator};
use std::sync::Arc;

pub struct ServerWait {
}

impl SimpleState for ServerWait {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/server_wait.ron", ());
        });
    }

    fn update(&mut self, _data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {

        Trans::None
    }
}
