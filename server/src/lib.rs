//! 40 Fields + House + Starts

use std::fmt::Debug;

use axum::extract::ws::Message;
use futures::stream::{SplitSink, SplitStream};
use serde_derive::{Deserialize, Serialize};

pub mod statemachine;

mod game;
pub use game::Game;

mod player;
pub use player::GamePlayer;

pub type RejoinMessage<SI, ST> = (uuid::Uuid, (SplitSink<SI, Message>, SplitStream<ST>));

#[derive(Debug, PartialEq)]
pub enum GameError {
    Disconnect,
    Other(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Figure {
    InStart,
    OnField { moved: usize },
    InHouse { pos: usize },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GameRequest {
    Roll,
    Move { figure: usize },
}

#[derive(Debug, Serialize, Deserialize)]
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
