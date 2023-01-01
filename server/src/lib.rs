//! 40 Fields + House + Starts

use std::fmt::Debug;

use serde_derive::{Deserialize, Serialize};

pub mod statemachine;

mod game;
pub use game::Game;

mod player;
pub use player::GamePlayer;

#[derive(Debug, PartialEq)]
pub enum GameError {
    Disconnect,
    Other(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Figure {
    InStart,
    OnField { moved: usize },
    InHouse { pos: usize },
}

#[derive(Debug, Deserialize)]
pub enum GameRequest {
    Roll,
    Move { figure: usize },
}

#[derive(Debug, Serialize)]
pub enum GameResponse {
    RejoinCode {
        game: uuid::Uuid,
        code: uuid::Uuid,
    },
    IndicatePlayer {
        player: usize,
        name: String,
        you: bool,
    },
    State {
        players: Vec<(String, [Figure; 4])>,
    },
    Turn,
    Rolled {
        value: usize,
        can_move: bool,
    },
    PlayerDone {
        player: usize,
    },
    GameDone {
        ranking: Vec<usize>,
    },
}
