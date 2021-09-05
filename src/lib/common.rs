use actix::dev::{MessageResponse, OneshotSender};
use actix::{Actor, Message};

#[derive(Debug, Clone)]
pub struct ResponseWrapper<T>(pub T);

impl<A, M, I> MessageResponse<A, M> for ResponseWrapper<I>
where
    A: Actor,
    M: Message<Result = I>,
    I: 'static,
{
    fn handle(self, _: &mut A::Context, tx: Option<OneshotSender<M::Result>>) {
        if let Some(tx) = tx {
            drop(tx.send(self.0));
        }
    }
}
