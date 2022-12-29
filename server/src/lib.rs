//! 40 Fields + House + Starts

use std::fmt::Debug;

use axum::extract::ws::{Message, WebSocket};
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use rand::{Rng, SeedableRng};
use serde_derive::{Deserialize, Serialize};

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

pub struct GamePlayer<Tx, Rx> {
    pub name: String,
    pub send: Tx,
    pub recv: Rx,
    pub figures: [Figure; 4],
    done: bool,
}

impl<Tx, Rx> GamePlayer<Tx, Rx>
where
    Tx: futures::Sink<Message> + Unpin,
    <Tx as futures::Sink<Message>>::Error: Debug,
{
    pub fn has_figures_in_start(&self) -> bool {
        self.figures.iter().any(|f| matches!(f, Figure::InStart))
    }
    pub fn has_figures_left(&self) -> bool {
        self.figures
            .iter()
            .any(|f| !matches!(f, Figure::InHouse { .. }))
    }
    pub fn has_figures_on_field(&self) -> bool {
        self.figures
            .iter()
            .any(|f| matches!(f, Figure::OnField { .. }))
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub async fn send_resp(&mut self, resp: &GameResponse) -> Result<(), ()> {
        let content = serde_json::to_string(resp).unwrap();
        match self.send.send(Message::Text(content)).await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("{:?}", e);
                Err(())
            }
        }
    }

    #[must_use]
    pub fn move_figure(&mut self, index: usize, amount: usize) -> Option<&Figure> {
        let figure = self.figures.get_mut(index)?;

        let n_state = match figure {
            Figure::InStart => Figure::InStart,
            Figure::OnField { moved } => {
                let target = *moved + amount;

                if target < 40 {
                    tracing::debug!(
                        "Move Figure {:?} on Field by {:?} from {:?} to {:?}",
                        index,
                        amount,
                        moved,
                        *moved + amount,
                    );

                    Figure::OnField { moved: target }
                } else {
                    let dif = target - 40;
                    tracing::debug!(
                        "Move Figure {:?} on Field by {:?} from {:?} to House {:?}",
                        index,
                        amount,
                        moved,
                        dif,
                    );

                    if dif < 4 {
                        Figure::InHouse { pos: dif }
                    } else {
                        Figure::OnField { moved: *moved }
                    }
                }
            }
            Figure::InHouse { pos } => {
                let target = *pos + amount;

                tracing::debug!(
                    "Move Figure {:?} in House by {:?} from {:?} to {:?}",
                    index,
                    amount,
                    pos,
                    target,
                );

                if target < 4 {
                    Figure::InHouse { pos: target }
                } else {
                    Figure::InHouse { pos: *pos }
                }
            }
        };

        if self.figures.iter().any(|f| f == &n_state) {
            return None;
        }

        let figure = self.figures.get_mut(index)?;
        *figure = n_state;

        Some(figure)
    }

    pub fn check_done(&mut self) -> bool {
        for figure in self.figures.iter() {
            if !matches!(figure, Figure::InHouse { .. }) {
                return false;
            }
        }

        self.done = true;
        return true;
    }
}

pub struct Game {
    pub players: Vec<GamePlayer<SplitSink<WebSocket, Message>, SplitStream<WebSocket>>>,
    pub next_player: usize,
    pub rng: rand::rngs::SmallRng,
    pub ranking: Vec<usize>,
}

impl Game {
    pub fn new<IP>(player_count: usize, players: IP) -> Self
    where
        IP: IntoIterator<Item = (String, WebSocket)>,
    {
        Game {
            players: players
                .into_iter()
                .map(|(name, s)| {
                    let (tx, rx) = s.split();
                    GamePlayer {
                        name,
                        send: tx,
                        recv: rx,
                        figures: [
                            Figure::InStart,
                            Figure::InStart,
                            Figure::InStart,
                            Figure::InStart,
                        ],
                        done: false,
                    }
                })
                .collect(),
            next_player: rand::thread_rng().gen_range(0..player_count),
            rng: rand::rngs::SmallRng::from_entropy(),
            ranking: Vec::new(),
        }
    }

    pub fn check_move(&mut self, player: usize) {
        let player_figures: Vec<_> = self
            .players
            .get(player)
            .unwrap()
            .figures
            .iter()
            .filter_map(|f| match f {
                Figure::OnField { moved } => Some((*moved + player * 10) % 40),
                _ => None,
            })
            .collect();

        tracing::trace!("Current Player Figure Positions {:?}", player_figures);

        for (pindex, player) in self
            .players
            .iter_mut()
            .enumerate()
            .filter(|(i, p)| *i != player)
        {
            for fig in player.figures.iter_mut() {
                let pos = match fig {
                    Figure::OnField { moved } => (*moved + pindex * 10) % 40,
                    _ => continue,
                };

                if player_figures.contains(&pos) {
                    *fig = Figure::InStart;
                    tracing::trace!("Figure {:?} of Player {} is done", fig, pindex);
                }
            }
        }
    }

    pub async fn send_state(&mut self) {
        let state = GameResponse::State {
            players: self
                .players
                .iter()
                .map(|p| (p.name.clone(), p.figures.clone()))
                .collect(),
        };
        for player in self.players.iter_mut() {
            match player.send_resp(&state).await {
                Ok(_) => {}
                Err(e) => {
                    todo!("{:?}", e)
                }
            };
        }
    }

    pub fn is_done(&self) -> bool {
        self.players.iter().all(|p| p.done)
    }
}
