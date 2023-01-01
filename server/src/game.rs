use axum::extract::ws::{Message, WebSocket};
use futures::{
    stream::{SplitSink, SplitStream},
    StreamExt,
};
use rand::{Rng, SeedableRng};

use crate::{Figure, GameError, GamePlayer, GameResponse};

pub struct Game<R> {
    id: uuid::Uuid,
    pub players: Vec<GamePlayer<SplitSink<WebSocket, Message>, SplitStream<WebSocket>>>,
    pub next_player: usize,
    pub rng: R,
    pub ranking: Vec<usize>,
}

impl Game<rand::rngs::SmallRng> {
    pub fn new<IP>(id: uuid::Uuid, player_count: usize, players: IP) -> Self
    where
        IP: IntoIterator<Item = (String, WebSocket)>,
    {
        Self::new_with_rng(
            id,
            player_count,
            players,
            rand::rngs::SmallRng::from_entropy(),
        )
    }
}

impl<R> Game<R>
where
    R: Rng,
{
    pub fn new_with_rng<IP>(id: uuid::Uuid, player_count: usize, players: IP, rng: R) -> Self
    where
        IP: IntoIterator<Item = (String, WebSocket)>,
    {
        Game {
            id,
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
                        rejoin_code: uuid::Uuid::new_v4(),
                    }
                })
                .collect(),
            next_player: rand::thread_rng().gen_range(0..player_count),
            rng,
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
            .filter(|(i, _)| *i != player)
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

    pub async fn send_rejoin_codes(&mut self) -> Result<(), GameError> {
        for player in self.players.iter_mut() {
            let msg = GameResponse::RejoinCode {
                game: self.id,
                code: player.rejoin_code,
            };

            player.send_resp(&msg).await?;
        }

        Ok(())
    }

    pub async fn send_state(&mut self) -> Result<(), GameError> {
        let state = GameResponse::State {
            players: self
                .players
                .iter()
                .map(|p| (p.name.clone(), p.figures.clone()))
                .collect(),
        };
        for player in self.players.iter_mut() {
            player.send_resp(&state).await?;
        }

        Ok(())
    }

    pub fn is_done(&self) -> bool {
        self.players.iter().all(|p| p.done)
    }
}
