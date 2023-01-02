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

/// The Errors that could be returned while running a Game
#[derive(Debug, PartialEq)]
pub enum GameError {
    /// A Player disconnected
    Disconnect,
    /// Other Errors occured
    Other(&'static str),
}

/// A Figure of a Player
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Figure {
    /// The Figure is still on the Start Field
    InStart,
    /// The Figure is already on the Field
    OnField {
        /// The Number of Fields that the Figure has already moved from it's starting position
        moved: usize,
    },
    /// The Figure is already in the House
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
