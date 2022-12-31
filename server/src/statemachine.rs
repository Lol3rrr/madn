use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;
use rand::Rng;

use crate::{Figure, Game, GameError, GameRequest, GameResponse};

pub enum GameState {
    WaitingForReconnect { prev_state: Box<GameState> },
    StartTurn,
    Rolled { value: usize },
    MoveToNextTurn,
    Done,
}

pub async fn step(
    prev: GameState,
    game: &mut Game,
    rejoin_rx: &mut tokio::sync::mpsc::UnboundedReceiver<(uuid::Uuid, WebSocket)>,
) -> Option<GameState> {
    let current_player = game.players.get_mut(game.next_player).unwrap();
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

                        game.send_state().await;

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

            let msg_text = match current_player.recv.next().await {
                Some(Ok(msg)) => match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => {
                        tracing::warn!("Player Disconnected");
                        return Some(GameState::WaitingForReconnect {
                            prev_state: Box::new(prev),
                        });
                    }
                    other => {
                        todo!("{:?}", other)
                    }
                },
                Some(Err(e)) => {
                    todo!("{:?}", e)
                }
                None => {
                    todo!()
                }
            };

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
                GameRequest::Move { .. } => {
                    todo!("Unexpected")
                }
            }
        }
        GameState::Rolled { value } => {
            let msg_text = match current_player.recv.next().await {
                Some(Ok(msg)) => match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => {
                        tracing::warn!("Player disconenct");
                        return Some(GameState::WaitingForReconnect {
                            prev_state: Box::new(GameState::Rolled { value }),
                        });
                    }
                    other => {
                        todo!("{:?}", other)
                    }
                },
                Some(Err(e)) => {
                    todo!("{:?}", e)
                }
                None => {
                    todo!()
                }
            };

            let req: GameRequest = match serde_json::from_str(&msg_text) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Error Message({:?}): {:?}", msg_text, e);
                    todo!()
                }
            };

            match req {
                GameRequest::Move { figure } => {
                    tracing::error!("Move Figure {:?} by Rolled {:?}", figure, value);

                    if current_player.move_figure(figure, value).is_none() {
                        tracing::warn!("Could not move Figure");
                    }
                    game.check_move(game.next_player);

                    game.send_state().await.unwrap();

                    GameState::MoveToNextTurn
                }
                GameRequest::Roll => {
                    todo!("Unexpected")
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
                tracing::warn!("Game is Done");

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
