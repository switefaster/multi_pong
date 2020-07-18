use amethyst::{SimpleState, GameData, StateData, StateEvent, SimpleTrans, Trans};
use amethyst::ecs::Entity;
use amethyst::input::{StringBindings, is_close_requested};
use amethyst::prelude::WorldExt;
use amethyst::ui::{UiCreator, UiFinder, UiEventType, UiText};

#[derive(Default)]
pub struct PlayerName {
    name_input: Option<Entity>,
    submit_button: Option<Entity>,
    notice_text: Option<Entity>,
}

#[derive(Default)]
pub struct PlayerNameResource {
    pub(crate) my_name: Option<String>,
    pub(crate) rival_name: Option<String>,
}

impl PlayerNameResource {
    pub fn new(my_name: Option<String>, rival_name: Option<String>) -> Self {
        Self {
            my_name,
            rival_name,
        }
    }
}

impl SimpleState for PlayerName {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let world = data.world;
        world.exec(|mut creator: UiCreator<'_>| {
            creator.create("ui/name_input.ron", ());
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
                }
                else {
                    Trans::None
                }
            }
            StateEvent::Ui(event) => {
                if let Some(submit) = self.submit_button {
                    if event.event_type == UiEventType::Click && event.target.id() == submit.id() {
                        if let Some(input) = self.name_input {
                            todo!("lifetime is to fix");
                            let mut storage = data.world.write_storage::<UiText>();
                            let text = storage.get(input).unwrap();
                            if text.text.is_empty() {
                                if let Some(notice) = self.notice_text {
                                    let mut notice = storage.get_mut(notice).unwrap();
                                    notice.text = String::from("UNEXPECTED EMPTY NAME");
                                    notice.color = [1.0, 0.0, 0.0, 1.0];
                                    return Trans::None
                                }
                            } else {
                                data.world.insert(PlayerNameResource::new(
                                    Some(text.text.clone()),
                                    None,
                                ));
                            }
                        }
                        return Trans::Push(Box::new(super::ModeSelect::default()))
                    }
                }
                Trans::None
            }
            _ => Trans::None
        }
    }

    fn update(&mut self, data: &mut StateData<'_, GameData<'_, '_>>) -> SimpleTrans {
        let StateData { world, ..} = data;

        if self.name_input.is_none() {
            world.exec(|finder: UiFinder| {
                self.name_input = finder.find("name_input");
            });
        }

        if self.submit_button.is_none() {
            world.exec(|finder: UiFinder| {
                self.submit_button = finder.find("submit");
            });
        }

        if self.notice_text.is_none() {
            world.exec(|finder: UiFinder| {
                self.notice_text = finder.find("notice");
            });
        }

        Trans::None
    }
}
