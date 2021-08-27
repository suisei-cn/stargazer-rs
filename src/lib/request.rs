use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use actix::dev::{Request, ToEnvelope};
use actix::MailboxError;
use actix::{Actor, Addr, Handler, Message};
use futures::ready;
use pin_project::pin_project;

#[must_use = "You have to await on request, call `blocking()` or `immediately()`, otherwise the message wont be delivered"]
pub enum MsgRequest<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    Initial { target: &'a Addr<A>, msg: Option<M> },
    Pending(#[pin] Request<A, M>),
    Gone,
}

impl<'a, A, M> MsgRequest<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    pub fn new(target: &'a Addr<A>, msg: M) -> Self {
        Self::Initial {
            target,
            msg: Some(msg),
        }
    }
}

impl<'a, A, M, O> MsgRequest<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + 'static,
    O: Send,
{
    /// Sends a message unconditionally, bypassing mailbox limit and ignore its response.
    #[allow(clippy::missing_panics_doc)]
    pub fn immediately(self) {
        // If it's been polled or ready, don't do anything.
        if let Self::Initial { target, msg } = self {
            target.do_send(msg.unwrap());
        }
    }

    /// Sends a message with blocking.
    ///
    /// # Panics
    /// Panics when called in an `actix-rt`(`tokio`) context, or this `MsgRequest` has already been polled halfway.
    #[allow(clippy::missing_errors_doc)]
    pub fn blocking(self) -> Result<O, MailboxError> {
        // If it's been polled or ready, don't do anything.
        if let Self::Initial { target, msg } = self {
            actix_rt::Runtime::new()
                .expect("unable to create runtime")
                .block_on(target.send(msg.unwrap()))
        } else {
            panic!("already been polled halfway")
        }
    }
}

impl<'a, A, M, O> Future for MsgRequest<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + Unpin,
    O: Send,
{
    type Output = Result<O, MailboxError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MsgRequestProj::Initial { target, msg } => {
                let fut = target.send(msg.take().unwrap());
                self.set(Self::Pending(fut));
                cx.waker().wake_by_ref(); // can be polled immediately to receive result
                Poll::Pending
            }
            MsgRequestProj::Pending(req) => {
                let r = ready!(req.poll(cx));
                self.set(Self::Gone); // seal this message request
                Poll::Ready(r)
            }
            MsgRequestProj::Gone => panic!("request done and can't be polled again"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::thread;

    use actix::{Actor, Context, Handler, Message, System};

    use super::MsgRequest;

    #[derive(Debug, Message)]
    #[rtype("usize")]
    struct Ping(usize, bool);

    #[derive(Debug, Message)]
    #[rtype("usize")]
    struct Query;

    #[derive(Debug, Default)]
    struct Echo(usize);

    impl Actor for Echo {
        type Context = Context<Self>;
    }

    impl Handler<Ping> for Echo {
        type Result = usize;

        fn handle(&mut self, msg: Ping, ctx: &mut Self::Context) -> Self::Result {
            self.0 = msg.0;
            if msg.1 {
                System::current().stop();
            }
            msg.0
        }
    }

    impl Handler<Query> for Echo {
        type Result = usize;

        fn handle(&mut self, msg: Query, ctx: &mut Self::Context) -> Self::Result {
            self.0
        }
    }

    #[actix::test]
    async fn must_req_do() {
        let addr = Echo::default().start();
        let req = MsgRequest::new(&addr, Ping(42, false));
        req.immediately();
        tokio::task::yield_now().await;
        assert_eq!(addr.send(Query).await.unwrap(), 42, "message not delivered");
    }

    #[actix::test]
    async fn must_req_await() {
        let addr = Echo::default().start();
        let req = MsgRequest::new(&addr, Ping(42, false));
        assert_eq!(req.await.unwrap(), 42, "response incorrect");
    }

    #[test]
    fn must_req_blocking() {
        let (tx, rx) = channel();

        // spawn in a new thread to avoid blocking main thread
        let sys_thread = thread::spawn(move || {
            let sys = System::new();
            sys.block_on(async {
                let addr = Echo::default().start();
                tx.send(addr).unwrap();
            });
            sys.run(); // join system
        });

        let addr = rx.recv().unwrap();
        let req = MsgRequest::new(&addr, Ping(42, true));
        assert_eq!(req.blocking().unwrap(), 42, "response incorrect");

        sys_thread.join().unwrap(); // sys thread should join and mustn't panic
    }
}
