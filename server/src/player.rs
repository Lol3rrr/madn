use std::fmt::Debug;

use axum::extract::ws::Message;
use futures::SinkExt;

use crate::{Figure, GameError, GameResponse};

#[derive(Debug)]
pub struct GamePlayer<Tx, Rx> {
    pub name: String,
    pub send: Tx,
    pub recv: Rx,
    pub figures: [Figure; 4],
    pub(crate) done: bool,
    pub(crate) rejoin_code: uuid::Uuid,
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
            .any(|f| matches!(f, Figure::OnField { .. } | Figure::InHouse { .. }))
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub async fn send_resp(&mut self, resp: &GameResponse) -> Result<(), GameError> {
        let content = serde_json::to_string(resp).unwrap();
        match self.send.send(Message::Text(content)).await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("{:?}", e);
                Err(GameError::Disconnect)
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
        true
    }
}
