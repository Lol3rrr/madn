use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Json, Path, State,
    },
    http::header,
    response::{Html, IntoResponse},
    routing::get,
    routing::post,
    Router,
};
use futures::StreamExt;
use server::{Game, GamePlayer, GameResponse, RejoinMessage};
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
    join: Arc<tokio::sync::mpsc::UnboundedSender<(String, WebSocket)>>,
    rejoin: Arc<tokio::sync::mpsc::UnboundedSender<RejoinMessage<WebSocket, WebSocket>>>,
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    players: usize,
}

#[tokio::main]
async fn main() {
    let machine_log = std::env::var("LOG_MACHINE").is_ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=trace".into()),
        )
        .with((machine_log).then(|| tracing_subscriber::fmt::layer().json()))
        .with((!machine_log).then(|| tracing_subscriber::fmt::layer().pretty()))
        .init();

    let state = Arc::new(AppState {
        sessions: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/style.css", get(style))
        .route("/dice.css", get(dice_style))
        .route("/create", post(create))
        .route("/join/:session/:name", get(join_handler))
        .route("/rejoin/:game/:key", get(rejoin_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn join_handler(
    Path((session, name)): Path<(Uuid, String)>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    tracing::trace!("Joinging {:?}", session);

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

    let target_tx = target_session.join.clone();

    ws.on_upgrade(|socket| async move {
        target_tx.send((name, socket)).expect("");
    })
}

async fn rejoin_handler(
    Path((game, key)): Path<(Uuid, Uuid)>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    tracing::trace!("Rejoin Game {:?} with {:?}", game, key);

    let sessions = state.sessions.lock().unwrap();

    let target_session = match sessions.get(&game) {
        Some(s) => s,
        None => {
            return axum::response::Response::builder()
                .status(axum::http::status::StatusCode::BAD_REQUEST)
                .body(axum::body::boxed(String::new()))
                .unwrap();
        }
    };

    let target_tx = target_session.rejoin.clone();

    ws.on_upgrade(move |socket| async move {
        target_tx.send((key, socket.split())).expect("");
    })
}

async fn create(
    State(state): State<Arc<AppState>>,
    Json(content): Json<CreateRequest>,
) -> impl IntoResponse {
    tracing::trace!("Create Game {:?}", content);

    let gameid = Uuid::new_v4();

    let (join_tx, join_rx) = tokio::sync::mpsc::unbounded_channel();
    let (rejoin_tx, rejoin_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(start_session(gameid, content.players, join_rx, rejoin_rx));

    {
        let mut games = state.sessions.lock().unwrap();
        games.insert(
            gameid,
            Session {
                join: Arc::new(join_tx),
                rejoin: Arc::new(rejoin_tx),
            },
        );
    }

    gameid.to_string()
}

#[tracing::instrument(skip(n_players, rejoin_players, player_count))]
async fn start_session(
    id: Uuid,
    player_count: usize,
    mut n_players: tokio::sync::mpsc::UnboundedReceiver<(String, WebSocket)>,
    mut rejoin_players: tokio::sync::mpsc::UnboundedReceiver<RejoinMessage<WebSocket, WebSocket>>,
) {
    tracing::debug!("Waiting for Players");

    let mut players = Vec::new();
    while let Some((name, ws)) = n_players.recv().await {
        // Add the newly connected Player
        players.push(GamePlayer::new(name, ws.split()));
        if players.len() == player_count {
            break;
        }

        // Send the information of all the currently joined Players around
        let all_players: Vec<_> = players
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.name.clone()))
            .collect();
        for (index, player) in players.iter_mut().enumerate() {
            for ind_player in all_players.iter() {
                let resp = GameResponse::IndicatePlayer {
                    player: ind_player.0,
                    name: ind_player.1.clone(),
                    you: ind_player.0 == index,
                };

                // We ignore errors at this stage, because we only do this for a better UX
                let _ = player.send_resp(&resp).await;
            }
        }
    }

    tracing::debug!("Starting Game");

    let mut game = Game::new(id, players);
    let mut gamestate = server::statemachine::GameState::StartTurn { attempt: 0 };

    // Here we use unwrap because we dont really have any good way to handle any potential issues here
    game.send_state().await.unwrap();
    game.indicate_players().await.unwrap();
    game.send_rejoin_codes().await.unwrap();

    let mut distr = rand::distributions::Uniform::new_inclusive(1, 6);

    loop {
        gamestate =
            match server::statemachine::step(gamestate, &mut game, &mut rejoin_players, &mut distr)
                .await
            {
                Some(gs) => gs,
                None => break,
            };

        tokio::task::yield_now().await;
    }
}

// Include utf-8 file at **compile** time.
async fn index() -> impl IntoResponse {
    #[cfg(not(debug_assertions))]
    return Html(std::include_str!("../assets/index.html"));

    #[cfg(debug_assertions)]
    return Html(std::fs::read_to_string("assets/index.html").unwrap());
}

async fn style() -> impl IntoResponse {
    #[cfg(not(debug_assertions))]
    let content = include_str!("../assets/style.css");
    #[cfg(debug_assertions)]
    let content = std::fs::read_to_string("assets/style.css").unwrap();

    ([(header::CONTENT_TYPE, "text/css")], content)
}

async fn dice_style() -> impl IntoResponse {
    #[cfg(not(debug_assertions))]
    let content = include_str!("../assets/dice.css");
    #[cfg(debug_assertions)]
    let content = std::fs::read_to_string("assets/dice.css").unwrap();

    ([(header::CONTENT_TYPE, "text/css")], content)
}
