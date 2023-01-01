use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;
use rand::Rng;

use crate::{Figure, Game, GameError, GameRequest, GameResponse};

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

pub enum GameState {
    WaitingForReconnect { prev_state: Box<GameState> },
    StartTurn,
    Rolled { value: usize },
    MoveToNextTurn,
    Done,
}

pub async fn step<R>(
    prev: GameState,
    game: &mut Game<R>,
    rejoin_rx: &mut tokio::sync::mpsc::UnboundedReceiver<(uuid::Uuid, WebSocket)>,
) -> Option<GameState>
where
    R: Rng,
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

    let next_state = match prev {
        GameState::WaitingForReconnect { prev_state } => match rejoin_rx.recv().await {
            Some((rejoin_key, new_socket)) => {
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

                        let (tx, rx) = new_socket.split();
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
        GameState::StartTurn => {
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

                    let value: usize = game.rng.gen_range(1..=6);

                    tracing::trace!("Rolled {} for Player {:?}", value, current_player.name);

                    let can_move = current_player.has_figures_on_field()
                        && !(value == 6 && current_player.has_figures_in_start())
                        && !current_player.figures.iter().any(|f| match f {
                            Figure::OnField { moved } => *moved == 0,
                            _ => false,
                        });

                    let resp = GameResponse::Rolled { value, can_move };
                    match current_player.send_resp(&resp).await {
                        Ok(_) => {}
                        Err(e) => {
                            todo!("{:?}", e);
                        }
                    };

                    if value == 6 {
                        if let Some(findex) =
                            current_player
                                .figures
                                .iter()
                                .enumerate()
                                .find(|(_, f)| match f {
                                    Figure::OnField { moved } => *moved == 0,
                                    _ => false,
                                })
                        {
                            if current_player.move_figure(findex.0, value).is_none() {
                                tracing::warn!("Figure could not be moved");
                            }
                            game.check_move(game.next_player);

                            game.send_state().await.unwrap();

                            GameState::StartTurn
                        } else if let Some(index) = current_player
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

                            GameState::StartTurn
                        } else {
                            GameState::Rolled { value }
                        }
                    } else if current_player.has_figures_on_field() {
                        if let Some(findex) =
                            current_player
                                .figures
                                .iter()
                                .enumerate()
                                .find(|(_, f)| match f {
                                    Figure::OnField { moved } => *moved == 0,
                                    _ => false,
                                })
                        {
                            if current_player.move_figure(findex.0, value).is_none() {
                                tracing::warn!("Figure could not be moved");
                            }
                            game.check_move(game.next_player);

                            game.send_state().await.unwrap();

                            GameState::MoveToNextTurn
                        } else {
                            GameState::Rolled { value }
                        }
                    } else {
                        GameState::MoveToNextTurn
                    }
                }
                other => {
                    tracing::error!("Unexpected {:?}", other);

                    GameState::StartTurn
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
                    game.check_move(game.next_player);

                    game.send_state().await.unwrap();

                    GameState::MoveToNextTurn
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

                GameState::StartTurn
            }
        }
        GameState::Done => {
            return None;
        }
    };

    Some(next_state)
}
