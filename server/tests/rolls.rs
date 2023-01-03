use std::vec;

use axum::extract::ws::Message;
use futures::StreamExt;
use server::{statemachine::GameState, Figure, Game, GameRequest};

use mocks::{DumbDistr, MockRand, MockSocket};
use tracing_test::traced_test;

#[tokio::test]
#[traced_test]
async fn roll6_with_multiple_instart() {
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
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

    game.next_player = 0;

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn { attempt: 0 };

    let (_rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 0 }, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn { attempt: 0 }, n_state);
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
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
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

    game.next_player = 0;

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn { attempt: 0 };

    game.players[0].figures[1] = Figure::OnField { moved: 10 };
    game.players[0].figures[2] = Figure::OnField { moved: 11 };
    game.players[0].figures[3] = Figure::OnField { moved: 12 };

    let (_rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::OnField { moved: 0 }, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn { attempt: 0 }, n_state);
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
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
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

    game.next_player = 0;

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn { attempt: 0 };

    game.players[0].figures[0] = Figure::OnField { moved: 1 };
    game.players[0].figures[1] = Figure::OnField { moved: 2 };
    game.players[0].figures[2] = Figure::OnField { moved: 3 };
    game.players[0].figures[3] = Figure::OnField { moved: 4 };

    let (_rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

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
    assert_eq!(GameState::StartTurn { attempt: 0 }, n_state);
    assert_eq!(0, game.next_player);
}

#[tokio::test]
#[traced_test]
async fn use3_attempts_no_onfield() {
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
        vec![
            (
                "test".to_string(),
                MockSocket::new(
                    tx1,
                    vec![
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                    ],
                )
                .split(),
            ),
            ("test2".to_string(), MockSocket::new(tx2, vec![]).split()),
        ],
        MockRand::new(vec![0, 2, 3, 0]),
    );

    game.next_player = 0;

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn { attempt: 0 };

    let (_rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn { attempt: 1 }, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn { attempt: 2 }, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::MoveToNextTurn, n_state);
    assert_eq!(0, game.next_player);
}

#[tokio::test]
#[traced_test]
async fn use3_attempts_already_done_inhouse() {
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
        vec![
            (
                "test".to_string(),
                MockSocket::new(
                    tx1,
                    vec![
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                    ],
                )
                .split(),
            ),
            ("test2".to_string(), MockSocket::new(tx2, vec![]).split()),
        ],
        MockRand::new(vec![0, 2, 3, 0]),
    );

    game.next_player = 0;
    game.players[0].figures[1] = Figure::InHouse { pos: 3 };

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn { attempt: 0 };

    let (_rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn { attempt: 1 }, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::StartTurn { attempt: 2 }, n_state);
    assert_eq!(0, game.next_player);

    let n_state = server::statemachine::step(n_state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::MoveToNextTurn, n_state);
    assert_eq!(0, game.next_player);
}

#[tokio::test]
#[traced_test]
async fn use3_attempts_not_done_inhouse() {
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

    let mut game: Game<_, MockSocket<Message>, MockSocket<Message>> = Game::new_with_rng(
        uuid::Uuid::new_v4(),
        vec![
            (
                "test".to_string(),
                MockSocket::new(
                    tx1,
                    vec![
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                        Message::Text(serde_json::to_string(&GameRequest::Roll).unwrap()),
                    ],
                )
                .split(),
            ),
            ("test2".to_string(), MockSocket::new(tx2, vec![]).split()),
        ],
        MockRand::new(vec![0, 2, 3, 0]),
    );

    game.next_player = 0;
    game.players[0].figures[1] = Figure::InHouse { pos: 2 };

    let mut distr = DumbDistr {};
    let state = GameState::StartTurn { attempt: 0 };

    let (_rejointx, mut rejoinrx) = tokio::sync::mpsc::unbounded_channel();

    let n_state = server::statemachine::step(state, &mut game, &mut rejoinrx, &mut distr)
        .await
        .expect("");

    assert_eq!(Figure::InStart, game.players[0].figures[0]);
    assert_eq!(GameState::Rolled { value: 1 }, n_state);
    assert_eq!(0, game.next_player);
}
