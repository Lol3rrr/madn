use futures::{Sink, Stream};

#[derive(Debug)]
pub struct MockSocket<C> {
    msgs: Vec<C>,
    tx: tokio::sync::mpsc::UnboundedSender<C>,
}

impl<C> Stream for MockSocket<C>
where
    C: Unpin,
{
    type Item = Result<C, axum::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let msg = self.msgs.remove(0);
        std::task::Poll::Ready(Some(Ok(msg)))
    }
}

impl<C> Sink<C> for MockSocket<C> {
    type Error = &'static str;

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: C) -> Result<(), Self::Error> {
        let _ = self.tx.send(item);
        Ok(())
    }
}

impl<C> MockSocket<C> {
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<C>, msgs: Vec<C>) -> Self {
        Self { tx, msgs }
    }
}
