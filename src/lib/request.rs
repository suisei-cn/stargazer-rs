use std::future::Future;
use std::mem;
use std::ops::Add;
use std::pin::Pin;
use std::task::{Context, Poll};

use actix::dev::{Request, ToEnvelope};
use actix::MailboxError;
use actix::{Actor, Addr, Handler, Message};
use futures::future::JoinAll;
use futures::ready;
use pin_project::pin_project;

trait RequestTrait: Future {
    /// Sends messages unconditionally, bypassing mailbox limit and ignore its response.
    fn immediately(self);
    /// Sends messages with blocking.
    ///
    /// # Panics
    /// Panics when called in an `actix-rt`(`tokio`) context, or any `MsgRequest` has already been polled halfway.
    fn blocking(self) -> <Self as Future>::Output;
}

trait BoxedRequestTrait: Future {
    /// Sends messages unconditionally, bypassing mailbox limit and ignore its response.
    fn immediately(self: Box<Self>);
    /// Sends messages with blocking.
    ///
    /// # Panics
    /// Panics when called in an `actix-rt`(`tokio`) context, or any `MsgRequest` has already been polled halfway.
    fn blocking(self: Box<Self>) -> <Self as Future>::Output;
}

impl<S: BoxedRequestTrait + Unpin> BoxedRequestTrait for Box<S> {
    fn immediately(self: Box<Self>) {
        self.immediately()
    }

    fn blocking(self: Box<Self>) -> Self::Output {
        self.blocking()
    }
}

#[must_use = "You have to await on requests, call `blocking()` or `immediately()`, otherwise the message wont be delivered"]
#[pin_project(project = MsgRequestsProj)]
pub enum MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send + Unpin + 'static,
    M::Result: Send,
{
    Init(Vec<MsgRequest<'a, A, M>>),
    Fut(#[pin] JoinAll<MsgRequest<'a, A, M>>),
    Gone,
}

impl<'a, A, M> MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send + Unpin + 'static,
    M::Result: Send,
{
    pub fn new(req: impl IntoIterator<Item = MsgRequest<'a, A, M>>) -> Self {
        Self::Init(req.into_iter().collect())
    }
}

impl<'a, A, M> Add for MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send + Unpin + 'static,
    M::Result: Send,
{
    type Output = MsgRequests<'a, A, M>;

    fn add(self, rhs: Self) -> Self::Output {
        if let (Self::Init(mut this), Self::Init(rhs)) = (self, rhs) {
            this.extend(rhs);
            Self::Init(this)
        } else {
            panic!("one of the `MsgRequests` was polled");
        }
    }
}

impl<'a, A, M> Add<MsgRequest<'a, A, M>> for MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send + Unpin + 'static,
    M::Result: Send,
{
    type Output = MsgRequests<'a, A, M>;

    fn add(self, rhs: MsgRequest<'a, A, M>) -> Self::Output {
        if let Self::Init(mut this) = self {
            this.push(rhs);
            Self::Init(this)
        } else {
            panic!("this `MsgRequest` was polled");
        }
    }
}

impl<'a, A, M, O> RequestTrait for MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + Unpin + 'static,
    O: Send,
{
    fn immediately(self) {
        if let Self::Init(l) = self {
            l.into_iter().for_each(RequestTrait::immediately);
        }
    }

    #[allow(clippy::missing_errors_doc)]
    fn blocking(self) -> Vec<Result<O, MailboxError>> {
        if matches!(self, Self::Init(_)) {
            actix_rt::Runtime::new()
                .expect("unable to create runtime")
                .block_on(self)
        } else {
            panic!("already been polled halfway")
        }
    }
}

impl<'a, A, M, O> BoxedRequestTrait for MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + Unpin + 'static,
    O: Send,
{
    fn immediately(self: Box<Self>) {
        (*self).immediately()
    }

    fn blocking(self: Box<Self>) -> Vec<Result<O, MailboxError>> {
        (*self).blocking()
    }
}

impl<'a, A, M, O> Future for MsgRequests<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + Unpin,
    O: Send,
{
    type Output = Vec<Result<O, MailboxError>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MsgRequestsProj::Init(reqs) => {
                let reqs = mem::take(reqs);
                let futs = futures::future::join_all(reqs);
                self.set(Self::Fut(futs));
                cx.waker().wake_by_ref(); // can be polled immediately to receive result
                Poll::Pending
            }
            MsgRequestsProj::Fut(fut) => {
                let res = ready!(fut.poll(cx));
                self.set(Self::Gone); // seal this MsgRequests
                Poll::Ready(res)
            }
            MsgRequestsProj::Gone => panic!("requests done and can't be polled again"),
        }
    }
}

#[must_use = "You have to await on request, call `blocking()` or `immediately()`, otherwise the message wont be delivered"]
#[pin_project(project = MsgRequestProj)]
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

impl<'a, A, M, O> RequestTrait for MsgRequest<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + Unpin + 'static,
    O: Send,
{
    fn immediately(self) {
        // If it's been polled or ready, don't do anything.
        if let Self::Initial { target, msg } = self {
            target.do_send(msg.unwrap());
        }
    }

    fn blocking(self) -> Result<O, MailboxError> {
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

impl<'a, A, M, O> BoxedRequestTrait for MsgRequest<'a, A, M>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message<Result = O> + Send + Unpin + 'static,
    O: Send,
{
    fn immediately(self: Box<Self>) {
        (*self).immediately()
    }

    fn blocking(self: Box<Self>) -> <Self as Future>::Output {
        (*self).blocking()
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
    use std::pin::Pin;
    use std::sync::mpsc::channel;
    use std::thread;

    use actix::{Actor, Context, Handler, Message, System};

    use super::{BoxedRequestTrait, MsgRequest, MsgRequests, RequestTrait};

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

    #[actix::test]
    async fn must_reqs_do() {
        let addr1 = Echo::default().start();
        let addr2 = Echo::default().start();
        let addr3 = Echo::default().start();
        let reqs = MsgRequests::new([
            MsgRequest::new(&addr1, Ping(41, false)),
            MsgRequest::new(&addr2, Ping(42, false)),
            MsgRequest::new(&addr3, Ping(43, false)),
        ]);
        reqs.immediately();
        tokio::task::yield_now().await;
        assert_eq!(
            addr1.send(Query).await.unwrap(),
            41,
            "message not delivered"
        );
        assert_eq!(
            addr2.send(Query).await.unwrap(),
            42,
            "message not delivered"
        );
        assert_eq!(
            addr3.send(Query).await.unwrap(),
            43,
            "message not delivered"
        );
    }

    #[actix::test]
    async fn must_reqs_await() {
        let addr = Echo::default().start();
        let reqs = MsgRequests::new([
            MsgRequest::new(&addr, Ping(41, false)),
            MsgRequest::new(&addr, Ping(42, false)),
            MsgRequest::new(&addr, Ping(43, false)),
        ]);
        assert_eq!(
            reqs.await
                .into_iter()
                .map(Result::unwrap)
                .collect::<Vec<_>>(),
            vec![41, 42, 43],
            "response incorrect"
        );
    }

    #[test]
    fn must_reqs_blocking() {
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
        let reqs = MsgRequests::new([MsgRequest::new(&addr, Ping(42, true))]);
        assert_eq!(
            reqs.blocking()
                .into_iter()
                .map(Result::unwrap)
                .collect::<Vec<_>>(),
            vec![42],
            "response incorrect"
        );

        sys_thread.join().unwrap(); // sys thread should join and mustn't panic
    }

    #[actix::test]
    async fn must_dyn_req_do() {
        let addr = Echo::default().start();
        let req: Box<dyn BoxedRequestTrait<Output = _>> =
            Box::new(MsgRequest::new(&addr, Ping(42, false)));
        req.immediately();
        tokio::task::yield_now().await;
        assert_eq!(addr.send(Query).await.unwrap(), 42, "message not delivered");
    }

    #[actix::test]
    async fn must_dyn_req_await() {
        let addr = Echo::default().start();
        let req: Pin<Box<dyn BoxedRequestTrait<Output = _>>> =
            Box::pin(MsgRequest::new(&addr, Ping(42, false)));
        assert_eq!(req.await.unwrap(), 42, "response incorrect");
    }
}
