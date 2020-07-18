mod in_game;
mod client_connect;
mod server_wait;
mod mode_select;
mod player_name;

pub use mode_select::ModeSelect;
pub use player_name::{PlayerName, PlayerNameResource};
pub use client_connect::ClientConnect;
pub use server_wait::ServerWait;
pub use in_game::InGame;

pub enum CurrentState {
    Ui,
    InGame,
}

impl Default for CurrentState {
    fn default() -> Self {
        Self::Ui
    }
}
