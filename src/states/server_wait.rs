use amethyst::{SimpleState, GameData, StateData};
use amethyst::ui::UiCreator;

pub struct ServerWait {
}

impl SimpleState for ServerWait {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/server_wait.ron", ());
        });
    }
}
