#![recursion_limit="256"]
mod network;
mod systems;
mod constants;
mod states;

use amethyst::{
    core::TransformBundle,
    input::{InputBundle, StringBindings},
    prelude::*,
    renderer::{
        plugins::{RenderFlat2D, RenderToWindow},
        types::DefaultBackend,
        RenderingBundle,
    },
    ui::{RenderUi, UiBundle},
    utils::application_root_dir,
};
use crate::systems::MultiPongBundle;

fn main() -> amethyst::Result<()> {
    amethyst::start_logger(Default::default());

    let app_root = application_root_dir()?;
    let display_config_path = app_root.join("config").join("display.ron");

    let binding_path = app_root.join("config").join("bindings.ron");
    let input_bundle =
        InputBundle::<StringBindings>::new()
            .with_bindings_from_file(binding_path)?;

    let game_data =
        GameDataBuilder::default()
            .with_bundle(
                RenderingBundle::<DefaultBackend>::new()
                    .with_plugin(
                        RenderToWindow::from_config_path(display_config_path)?
                            .with_clear([0.0, 0.0, 0.0, 1.0]),
                    )
                    .with_plugin(
                        RenderFlat2D::default())
                    .with_plugin(RenderUi::default()),
            )?
            .with_bundle(TransformBundle::new())?
            .with_bundle(input_bundle)?
            .with_bundle(UiBundle::<StringBindings>::new())?
            .with_bundle(MultiPongBundle)?;

    let assets_dir = app_root.join("assets");
    let mut game = Application::new(assets_dir, states::PlayerName::default(), game_data)?;
    game.run();

    Ok(())
}
