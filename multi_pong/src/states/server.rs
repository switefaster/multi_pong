use crate::network::{init_server, NETWORK};
use crate::states::CurrentState;
use amethyst::ecs::Entity;
use amethyst::input::{is_close_requested, StringBindings};
use amethyst::prelude::WorldExt;
use amethyst::ui::{UiCreator, UiEventType, UiFinder, UiText};
use amethyst::{GameData, SimpleState, SimpleTrans, StateData, StateEvent, Trans};

#[derive(Default)]
pub struct ServerPortInput {
    button: Option<Entity>,
    input: Option<Entity>,
}

impl SimpleState for ServerPortInput {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/server_port_input.ron", ());
        });
    }

    fn on_pause(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        data.world.delete_all();
    }

    fn handle_event(
        &mut self,
        data: StateData<'_, GameData<'_, '_>>,
        event: StateEvent<StringBindings>,
    ) -> SimpleTrans {
        match event {
            StateEvent::Window(event) => {
                if is_close_requested(&event) {
                    Trans::Quit
                } else {
                    Trans::None
                }
            }
            StateEvent::Ui(event) => {
                if let Some(button) = self.button {
                    if event.event_type == UiEventType::Click && event.target == button {
                        println!("click");
                        if let Some(input) = self.input {
                            println!("click2");
                            let storage = data.world.write_storage::<UiText>();
                            let text = storage.get(input).unwrap();
                            let port = text.text.clone();
                            std::mem::drop(storage);
                            init_server(port.parse().unwrap());
                        }
                    }
                }
                Trans::None
            }
            _ => Trans::None,
        }
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        let StateData { world, .. } = data;

        if self.button.is_none() {
            world.exec(|finder: UiFinder| {
                self.button = finder.find("publish");
            });
        }

        if self.input.is_none() {
            world.exec(|finder: UiFinder| {
                self.input = finder.find("port");
            });
        }

        if let Ok(mut network) = NETWORK.try_lock() {
            if let Some(network) = network.take() {
                data.world.insert(network);
                return Trans::Push(Box::new(ServerWait));
            }
        }
        Trans::None
    }
}

pub struct ServerWait;

impl SimpleState for ServerWait {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/server_wait.ron", ());
        });
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
