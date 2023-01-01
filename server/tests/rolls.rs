use std::vec;

use axum::extract::ws::Message;
use futures::StreamExt;
use server::{statemachine::GameState, Figure, Game, GameRequest};

use mocks::{DumbDistr, MockRand, MockSocket};
use tracing_test::traced_test;

#[tokio::test]
#[traced_test]
async fn roll6_with_multiple_instart() {
    let (tx1, rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
        1,
        vec![
            (
                "test".to_string(),
                MockSocket::new(
                    tx1,
                    vec![
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                    ],
                )
                .split(),
            ),
            ("test2".to_string(), MockSocket::new(tx2, vec![]).split()),
        ],
        MockRand::new(vec![5, 3]),
    );

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn;

    let (rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 0 }, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 4 }, game.players[0].figures[0]);
    assert_eq!(GameState::MoveToNextTurn, n_state);
    assert_eq!(0, game.next_player);
}

#[tokio::test]
#[traced_test]
async fn roll6_with_one_instart() {
    let (tx1, rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
        1,
        vec![
            (
                "test".to_string(),
                MockSocket::new(
                    tx1,
                    vec![
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                    ],
                )
                .split(),
            ),
            ("test2".to_string(), MockSocket::new(tx2, vec![]).split()),
        ],
        MockRand::new(vec![5, 3]),
    );

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn;

    game.players[0].figures[1] = Figure::OnField { moved: 10 };
    game.players[0].figures[2] = Figure::OnField { moved: 11 };
    game.players[0].figures[3] = Figure::OnField { moved: 12 };

    let (rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 0 }, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 0 }, game.players[0].figures[0]);
    assert_eq!(GameState::Rolled { value: 4 }, n_state);
    assert_eq!(0, game.next_player);
}

#[tokio::test]
#[traced_test]
async fn roll6_with_only_field() {
    let (tx1, rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
        1,
        vec![
            (
                "test".to_string(),
                MockSocket::new(
                    tx1,
                    vec![
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(
                            serde_json::to_string(&GameRequest::Move { figure: 0 }).unwrap(),
                        ),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                    ],
                )
                .split(),
            ),
            ("test2".to_string(), MockSocket::new(tx2, vec![]).split()),
        ],
        MockRand::new(vec![5, 3]),
    );

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn;

    game.players[0].figures[0] = Figure::OnField { moved: 1 };
    game.players[0].figures[1] = Figure::OnField { moved: 2 };
    game.players[0].figures[2] = Figure::OnField { moved: 3 };
    game.players[0].figures[3] = Figure::OnField { moved: 4 };

    let (rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 1 }, game.players[0].figures[0]);
    assert_eq!(GameState::Rolled { value: 6 }, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 7 }, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn, n_state);
    assert_eq!(0, game.next_player);
}
