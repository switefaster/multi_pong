use amethyst::{SimpleState, GameData, StateData, StateEvent, SimpleTrans, Trans};
use amethyst::ecs::Entity;
use amethyst::input::{StringBindings, is_close_requested};
use amethyst::prelude::WorldExt;
use amethyst::ui::{UiCreator, UiFinder, UiEventType, UiText};
use crate::network::{create_client_background_loop, NetworkCommunication, Instruction, Packet};
use crate::states::{CurrentState, PlayerNameResource};

#[derive(Default)]
pub struct ClientAddrInput {
    button: Option<Entity>,
    input: Option<Entity>,
}

impl SimpleState for ClientAddrInput {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/client_addr_input.ron", ());
        });
    }

    fn on_pause(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        data.world.delete_all();
    }

    fn handle_event(&mut self, data: StateData<'_, GameData<'_, '_>>, event: StateEvent<StringBindings>) -> SimpleTrans {
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
                        if let Some(input) = self.input {
                            let storage = data.world.write_storage::<UiText>();
                            let text = storage.get(input).unwrap();
                            let address = text.text.clone();
                            std::mem::drop(storage);
                            data.world.insert(create_client_background_loop(address + ":4001"));
                            return Trans::Push(Box::new(ClientConnecting));
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

        if self.button.is_none() {
            world.exec(|finder: UiFinder| {
                self.button = finder.find("connect");
            });
        }

        if self.input.is_none() {
            world.exec(|finder: UiFinder| {
                self.input = finder.find("host_input");
            });
        }

        Trans::None
    }
}

pub struct ClientConnecting;

impl SimpleState for ClientConnecting {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/connecting.ron", ());
        });
        let mut comm = world.write_resource::<NetworkCommunication>();
        let name = world.read_resource::<PlayerNameResource>();
        if let Some(ref mut sender) = comm.sender {
            sender.unbounded_send(Instruction::SendPacket(Packet::Handshake {
                player_name: name.my_name.clone().unwrap(),
            })).unwrap();
        }
    }

    fn on_pause(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        data.world.delete_all();
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        let state = data.world.read_resource::<CurrentState>();
        if let CurrentState::InGame = *state {
            Trans::Push(Box::new(super::InGame))
        } else {
            Trans::None
        }
    }
}
