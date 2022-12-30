use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Json, Path, State,
    },
    response::{Html, IntoResponse},
    routing::get,
    routing::post,
    Router,
};
use futures::stream::StreamExt;
use rand::Rng;
use server::{Figure, Game, GameRequest, GameResponse};
use std::{
    collections::HashMap,
    fmt::Debug,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use serde_derive::Deserialize;

#[derive(Debug)]
struct AppState {
    sessions: Mutex<HashMap<Uuid, Session>>,
}

#[derive(Debug)]
struct Session {
    sender: Arc<tokio::sync::mpsc::UnboundedSender<(String, WebSocket)>>,
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    players: usize,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = Arc::new(AppState {
        sessions: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/create", post(create))
        .route("/websocket/:session/:name", get(websocket_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn websocket_handler(
    Path((session, name)): Path<(Uuid, String)>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    tracing::trace!("Websocket connection for {:?}", session);

    let sessions = state.sessions.lock().unwrap();

    let target_session = match sessions.get(&session) {
        Some(s) => s,
        None => {
            return axum::response::Response::builder()
                .status(axum::http::status::StatusCode::BAD_REQUEST)
                .body(axum::body::boxed(String::new()))
                .unwrap();
        }
    };

    let target_tx = target_session.sender.clone();

    ws.on_upgrade(|socket| async move {
        target_tx.send((name, socket));
    })
}

async fn create(
    State(state): State<Arc<AppState>>,
    Json(content): Json<CreateRequest>,
) -> impl IntoResponse {
    tracing::trace!("Create Game {:?}", content);

    let gameid = Uuid::new_v4();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(start_session(gameid, content.players, rx));

    {
        let mut games = state.sessions.lock().unwrap();
        games.insert(
            gameid,
            Session {
                sender: Arc::new(tx),
            },
        );
    }

    gameid.to_string()
}

enum GameState {
    StartTurn,
    Rolled { value: usize },
    MoveToNextTurn,
    Done,
}

#[tracing::instrument(skip(n_players, player_count))]
async fn start_session(
    id: Uuid,
    player_count: usize,
    mut n_players: tokio::sync::mpsc::UnboundedReceiver<(String, WebSocket)>,
) {
    tracing::debug!("Waiting for Players");

    let mut players = Vec::new();
    while let Some((name, ws)) = n_players.recv().await {
        players.push((name, ws));
        if players.len() == player_count {
            break;
        }
    }

    tracing::debug!("Starting Game");

    let mut game = Game::new(player_count, players);
    game.send_state().await;

    let mut gamestate = GameState::StartTurn;

    let player_indicators: Vec<_> = game
        .players
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.name.clone()))
        .collect();
    for (index, player) in game.players.iter_mut().enumerate() {
        for (name_index, name) in player_indicators.iter() {
            player
                .send_resp(&GameResponse::IndicatePlayer {
                    player: *name_index,
                    name: name.clone(),
                    you: index == *name_index,
                })
                .await;
        }
    }

    loop {
        let current_player = game.players.get_mut(game.next_player).unwrap();
        tracing::debug!(
            "Player {}({}) is the currently running player",
            game.next_player,
            current_player.name
        );

        let next_state = match gamestate {
            GameState::StartTurn => {
                match current_player.send_resp(&GameResponse::Turn).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("{:?}", e);
                        todo!()
                    }
                };

                let msg_text = match current_player.recv.next().await {
                    Some(Ok(msg)) => match msg {
                        Message::Text(t) => t,
                        Message::Close(c) => {
                            tracing::error!("Close: {:?}", c);
                            todo!()
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

                                game.send_state().await;

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
                                game.send_state().await;

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

                                game.send_state().await;

                                GameState::MoveToNextTurn
                            } else {
                                GameState::Rolled { value }
                            }
                        } else {
                            GameState::MoveToNextTurn
                        }
                    }
                    GameRequest::Move { figure } => {
                        todo!("Unexpected")
                    }
                }
            }
            GameState::Rolled { value } => {
                let msg_text = match current_player.recv.next().await {
                    Some(Ok(msg)) => match msg {
                        Message::Text(t) => t,
                        Message::Close(c) => {
                            tracing::error!("[{}] Close: {:?}", id, c);
                            todo!()
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

                        game.send_state().await;

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
                        player.send_resp(&done_msg).await;
                    }
                }

                if game.is_done() {
                    tracing::warn!("Game is Done");

                    let done_msg = GameResponse::GameDone {
                        ranking: game.ranking.clone(),
                    };
                    for player in game.players.iter_mut() {
                        player.send_resp(&done_msg).await;
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
                return;
            }
        };

        gamestate = next_state;

        tokio::task::yield_now().await;
    }
}

// Include utf-8 file at **compile** time.
async fn index() -> Html<&'static str> {
    Html(std::include_str!("../chat.html"))
}
