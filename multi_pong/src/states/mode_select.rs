use amethyst::{SimpleState, GameData, StateData, StateEvent, SimpleTrans, Trans};
use amethyst::ecs::Entity;
use amethyst::input::{StringBindings, is_close_requested};
use amethyst::prelude::WorldExt;
use amethyst::ui::{UiCreator, UiFinder, UiEventType};

#[derive(Default)]
pub struct ModeSelect {
    server_button: Option<Entity>,
    client_button: Option<Entity>,
}

impl SimpleState for ModeSelect {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/mode_select.ron", ());
        });
    }

    fn on_pause(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        data.world.delete_all();
    }

    fn handle_event(&mut self, _data: StateData<'_, GameData<'_, '_>>, event: StateEvent<StringBindings>) -> SimpleTrans {
        match event {
            StateEvent::Window(event) => {
                if is_close_requested(&event) {
                    Trans::Quit
                }
                else {
                    Trans::None
                }
            }
            StateEvent::Ui(event) => {
                if event.event_type == UiEventType::Click {
                    if let Some(button) = self.client_button {
                        if button == event.target {
                            return Trans::Push(Box::new(super::ClientAddrInput::default()))
                        }
                    }
                    if let Some(button) = self.server_button {
                        if button == event.target {
                            return Trans::Push(Box::new(super::ServerWait))
                        }
                    }
                }
                Trans::None
            }
            _ => Trans::None
        }
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        let StateData { world, ..} = data;

        if self.server_button.is_none() {
            world.exec(|finder: UiFinder| {
                self.server_button = finder.find("as_server");
            });
        }

        if self.client_button.is_none() {
            world.exec(|finder: UiFinder| {
                self.client_button = finder.find("as_client");
            });
        }

        Trans::None
    }
}
