use std::fmt::Debug;

use axum::extract::ws::Message;
use futures::{Sink, Stream, StreamExt};
use rand::Rng;

use crate::{Figure, Game, GameError, GameRequest, GameResponse, RejoinMessage};

macro_rules! recv_msg {
    ($receiver:expr, $prev_state:expr) => {
        match $receiver.next().await {
            Some(Ok(msg)) => match msg {
                Message::Text(t) => t,
                Message::Close(_) => {
                    tracing::warn!("Player Disconnected");
                    return Some(GameState::WaitingForReconnect {
                        prev_state: $prev_state,
                    });
                }
                other => {
                    todo!("{:?}", other)
                }
            },
            Some(Err(e)) => {
                tracing::error!("Error receiving {:?}", e);
                return Some(GameState::WaitingForReconnect {
                    prev_state: $prev_state,
                });
            }
            None => {
                todo!()
            }
        }
    };
}

#[derive(Debug, PartialEq)]
pub enum GameState {
    WaitingForReconnect { prev_state: Box<GameState> },
    StartTurn { attempt: usize },
    Rolled { value: usize },
    MoveToNextTurn,
    Done,
}

pub async fn step<R, SI, ST, D>(
    prev: GameState,
    game: &mut Game<R, SI, ST>,
    rejoin_rx: &mut tokio::sync::mpsc::UnboundedReceiver<RejoinMessage<SI, ST>>,
    distr: &mut D,
) -> Option<GameState>
where
    R: Rng,
    SI: Sink<Message>,
    <SI as futures::Sink<Message>>::Error: Debug,
    ST: Stream<Item = Result<Message, axum::Error>>,
    D: rand::distributions::Distribution<usize>,
{
    let current_player = game
        .players
        .get_mut(game.next_player)
        .expect("We always know that our index is within bounds of the Player Vec");
    tracing::debug!(
        "Player {}({}) is the currently running player",
        game.next_player,
        current_player.name
    );

    tracing::trace!("Current State {:?}", prev);

    let next_state = match prev {
        GameState::WaitingForReconnect { prev_state } => match rejoin_rx.recv().await {
            Some((rejoin_key, (tx, rx))) => {
                let player_index_res = game.players.iter().enumerate().find_map(|(i, p)| {
                    if p.rejoin_code == rejoin_key {
                        Some(i)
                    } else {
                        None
                    }
                });

                match player_index_res {
                    Some(player_index) => {
                        let rejoined_player = game
                            .players
                            .get_mut(player_index)
                            .expect("We found the index by searching the same array");

                        rejoined_player.send = tx;
                        rejoined_player.recv = rx;

                        // We ignore these results because if any of the connections fail again, we will just re-enter this
                        // state again later on
                        let _ = game.send_state().await;
                        let _ = game.indicate_players().await;

                        *prev_state
                    }
                    None => {
                        tracing::warn!("Unknown Rejoin Key");
                        GameState::WaitingForReconnect { prev_state }
                    }
                }
            }
            None => {
                todo!()
            }
        },
        GameState::StartTurn { attempt } => {
            match current_player.send_resp(&GameResponse::Turn).await {
                Ok(_) => {}
                Err(e) => match e {
                    GameError::Disconnect => {
                        tracing::warn!("Player disconnected");
                        return Some(GameState::WaitingForReconnect {
                            prev_state: Box::new(prev),
                        });
                    }
                    GameError::Other(reason) => {
                        todo!("Other Error: {:?}", reason)
                    }
                },
            };

            let msg_text = recv_msg!(current_player.recv, Box::new(prev));

            let req: GameRequest = match serde_json::from_str(&msg_text) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Error Message({:?}): {:?}", msg_text, e);
                    todo!()
                }
            };

            match req {
                GameRequest::Roll => {
                    tracing::trace!("Rolling for Player {:?}", current_player.name);

                    let value: usize = distr.sample(&mut game.rng);

                    tracing::trace!("Rolled {} for Player {:?}", value, current_player.name);

                    let other_figures_instart = current_player
                        .figures
                        .iter()
                        .any(|f| matches!(f, Figure::InStart));

                    let can_move = current_player.has_figures_on_field()
                        && !(value == 6 && current_player.has_figures_in_start())
                        && !current_player.figures.iter().any(|f| match f {
                            Figure::OnField { moved } => *moved == 0 && other_figures_instart,
                            _ => false,
                        });

                    let resp = GameResponse::Rolled { value, can_move };
                    match current_player.send_resp(&resp).await {
                        Ok(_) => {}
                        Err(e) => match e {
                            GameError::Disconnect => {
                                return Some(prev);
                            }
                            GameError::Other(reason) => {
                                tracing::error!("Error sending Response {:?}", reason);
                                todo!()
                            }
                        },
                    };

                    let figure_startfield_index_res = current_player
                        .figures
                        .iter()
                        .enumerate()
                        .find(|(_, f)| match f {
                            Figure::OnField { moved } => *moved == 0 && other_figures_instart,
                            _ => false,
                        });

                    if let Some(findex) = figure_startfield_index_res {
                        if current_player.move_figure(findex.0, value).is_none() {
                            tracing::warn!("Figure could not be moved");
                        }

                        game.check_move(game.next_player);
                        game.send_state().await.unwrap();

                        if value == 6 {
                            return Some(GameState::StartTurn { attempt: 0 });
                        } else {
                            return Some(GameState::MoveToNextTurn);
                        }
                    }

                    if value == 6 {
                        if let Some(index) =
                            current_player
                                .figures
                                .iter()
                                .enumerate()
                                .find_map(|(i, f)| match f {
                                    Figure::InStart => Some(i),
                                    _ => None,
                                })
                        {
                            *current_player
                                .figures
                                .get_mut(index)
                                .expect("We just got the index by iterating over the list") =
                                Figure::OnField { moved: 0 };

                            tracing::trace!(
                                "Moved Figure {} out of Start for Player {:?}",
                                index,
                                current_player.name
                            );

                            game.check_move(game.next_player);
                            game.send_state().await.unwrap();

                            GameState::StartTurn { attempt: 0 }
                        } else {
                            GameState::Rolled { value }
                        }
                    } else if current_player.has_figures_on_field() {
                        GameState::Rolled { value }
                    } else if attempt >= 2 {
                        GameState::MoveToNextTurn
                    } else {
                        GameState::StartTurn {
                            attempt: attempt + 1,
                        }
                    }
                }
                other => {
                    tracing::error!("Unexpected {:?}", other);

                    GameState::StartTurn { attempt }
                }
            }
        }
        GameState::Rolled { value } => {
            let msg_text = recv_msg!(current_player.recv, Box::new(GameState::Rolled { value }));

            let req: GameRequest = match serde_json::from_str(&msg_text) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Error Message({:?}): {:?}", msg_text, e);
                    todo!()
                }
            };

            match req {
                GameRequest::Move { figure } => {
                    tracing::trace!("Move Figure {:?} by Rolled {:?}", figure, value);

                    if current_player.move_figure(figure, value).is_none() {
                        tracing::warn!("Could not move Figure");
                    }
                    let player_done = current_player.check_done();

                    game.check_move(game.next_player);

                    game.send_state().await.unwrap();

                    if value == 6 && !player_done {
                        GameState::StartTurn { attempt: 0 }
                    } else {
                        GameState::MoveToNextTurn
                    }
                }
                other => {
                    tracing::error!("Unexpected {:?}", other);

                    GameState::Rolled { value }
                }
            }
        }
        GameState::MoveToNextTurn => {
            if !current_player.is_done() && current_player.check_done() {
                tracing::trace!("Player {:?} is Done", game.next_player);

                game.ranking.push(game.next_player);

                let done_msg = GameResponse::PlayerDone {
                    player: game.next_player,
                };
                for player in game.players.iter_mut() {
                    player.send_resp(&done_msg).await.unwrap();
                }
            }

            if game.is_done() {
                tracing::debug!("Game is Done");

                let done_msg = GameResponse::GameDone {
                    ranking: game.ranking.clone(),
                };
                for player in game.players.iter_mut() {
                    player.send_resp(&done_msg).await.unwrap();
                }

                GameState::Done
            } else {
                for _ in 0..game.players.len() {
                    game.next_player = (game.next_player + 1) % game.players.len();

                    if !game.players[game.next_player].is_done() {
                        break;
                    }
                }

                GameState::StartTurn { attempt: 0 }
            }
        }
        GameState::Done => {
            return None;
        }
    };

    Some(next_state)
}
