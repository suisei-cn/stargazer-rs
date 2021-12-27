use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct ArbiterHandler {
    stop_count: usize,
    rx: UnboundedReceiver<()>,
}

impl ArbiterHandler {
    pub fn new(stop_count: usize, rx: UnboundedReceiver<()>) -> Self {
        Self { stop_count, rx }
    }
}

impl Future for ArbiterHandler {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match ready!(self.rx.poll_recv(cx)) {
                None => return Poll::Ready(()), // event channel closed
                Some(()) => {
                    self.stop_count -= 1;
                    if self.stop_count == 0 {
                        return Poll::Ready(()); // all arbs are stopped
                    }
                }
            }
        }
    }
}
