use std::future::Future;
use std::mem;
use std::ops::Add;
use std::pin::Pin;
use std::task::{Context, Poll};

use actix::dev::{Request, ToEnvelope};
use actix::MailboxError;
use actix::{Actor, Addr, Handler, Message};
use futures::future::{Join, JoinAll};
use futures::ready;
use pin_project::pin_project;

pub trait RequestTrait: Future {
    /// Sends messages unconditionally, bypassing mailbox limit and ignore its response.
    fn immediately(self);
    /// Sends messages with blocking.
    ///
    /// # Panics
    /// Panics when called in an `actix-rt`(`tokio`) context, or any `MsgRequest` has already been polled halfway.
    fn blocking(self) -> <Self as Future>::Output;

    fn join<R: RequestTrait + Sized>(self, other: R) -> MsgRequestTuple<Self, R>
    where
        Self: Sized,
    {
        MsgRequestTuple::new(self, other)
    }

    fn map<O, F: FnOnce(Self::Output) -> O>(self, f: F) -> MsgRequestMap<Self, O, F>
    where
        Self: Sized,
    {
        MsgRequestMap::new(self, f)
    }
}

#[must_use = "You have to await on requests, call `blocking()` or `immediately()`, otherwise the message wont be delivered"]
#[pin_project(project = MsgRequestMapProj)]
pub enum MsgRequestMap<R, O, F>
where
    R: Future,
    F: FnOnce(R::Output) -> O,
{
    Init(#[pin] R, Option<F>),
    Gone,
}

impl<R, O, F> MsgRequestMap<R, O, F>
where
    R: Future,
    F: FnOnce(R::Output) -> O,
{
    pub fn new(r: R, f: F) -> Self {
        Self::Init(r, Some(f))
    }
}

impl<R, O, F> RequestTrait for MsgRequestMap<R, O, F>
where
    R: RequestTrait,
    F: FnOnce(R::Output) -> O,
{
    fn immediately(self) {
        if let Self::Init(r, _) = self {
            r.immediately();
        }
    }

    fn blocking(self) -> <Self as Future>::Output {
        if matches!(self, Self::Init(_, _)) {
            actix_rt::Runtime::new()
                .expect("unable to create runtime")
                .block_on(self)
        } else {
            panic!("already been polled halfway")
        }
    }
}

impl<R, O, F> Future for MsgRequestMap<R, O, F>
where
    R: Future,
    F: FnOnce(R::Output) -> O,
{
    type Output = O;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MsgRequestMapProj::Init(req, f) => {
                let ready = ready!(req.poll(cx));
                let f = f.take().unwrap();
                self.set(Self::Gone); // seal this MsgRequestTuple
                Poll::Ready(f(ready))
            }
            MsgRequestMapProj::Gone => panic!("requests done and can't be polled again"),
        }
    }
}

#[must_use = "You have to await on requests, call `blocking()` or `immediately()`, otherwise the message wont be delivered"]
#[pin_project(project = MsgRequestTupleProj)]
pub enum MsgRequestTuple<R1: Future, R2: Future> {
    Init(Option<R1>, Option<R2>),
    Fut(#[pin] Join<R1, R2>),
    Gone,
}

impl<R1: Future, R2: Future> MsgRequestTuple<R1, R2> {
    pub fn new(r1: R1, r2: R2) -> Self {
        Self::Init(Some(r1), Some(r2))
    }
}

impl<R1: RequestTrait, R2: RequestTrait> RequestTrait for MsgRequestTuple<R1, R2> {
    fn immediately(self) {
        if let Self::Init(r1, r2) = self {
            r1.unwrap().immediately();
            r2.unwrap().immediately();
        }
    }

    fn blocking(self) -> (R1::Output, R2::Output) {
        if matches!(self, Self::Init(_, _)) {
            actix_rt::Runtime::new()
                .expect("unable to create runtime")
                .block_on(self)
        } else {
            panic!("already been polled halfway")
        }
    }
}

impl<R1: RequestTrait, R2: RequestTrait> Future for MsgRequestTuple<R1, R2> {
    type Output = (R1::Output, R2::Output);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MsgRequestTupleProj::Init(r1, r2) => {
                let r1 = r1.take().unwrap();
                let r2 = r2.take().unwrap();
                let futs = futures::future::join(r1, r2);
                self.set(Self::Fut(futs));
                cx.waker().wake_by_ref(); // can be polled immediately to receive result
                Poll::Pending
            }
            MsgRequestTupleProj::Fut(fut) => {
                let res = ready!(fut.poll(cx));
                self.set(Self::Gone); // seal this MsgRequestTuple
                Poll::Ready(res)
            }
            MsgRequestTupleProj::Gone => panic!("requests done and can't be polled again"),
        }
    }
}

#[must_use = "You have to await on requests, call `blocking()` or `immediately()`, otherwise the message wont be delivered"]
#[pin_project(project = MsgRequestVecProj)]
pub enum MsgRequestVec<R: Future> {
    Init(Vec<R>),
    Fut(#[pin] JoinAll<R>),
    Gone,
}

impl<R: Future> MsgRequestVec<R> {
    pub fn new(req: impl IntoIterator<Item = R>) -> Self {
        Self::Init(req.into_iter().collect())
    }
}

impl<R: Future> Add for MsgRequestVec<R> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        if let (Self::Init(mut this), Self::Init(rhs)) = (self, rhs) {
            this.extend(rhs);
            Self::Init(this)
        } else {
            panic!("one of the `MsgRequests` was polled");
        }
    }
}

impl<R: Future> Add<R> for MsgRequestVec<R> {
    type Output = Self;

    fn add(self, rhs: R) -> Self::Output {
        if let Self::Init(mut this) = self {
            this.push(rhs);
            Self::Init(this)
        } else {
            panic!("this `MsgRequest` was polled");
        }
    }
}

impl<R: RequestTrait> RequestTrait for MsgRequestVec<R> {
    fn immediately(self) {
        if let Self::Init(l) = self {
            l.into_iter().for_each(RequestTrait::immediately);
        }
    }

    #[allow(clippy::missing_errors_doc)]
    fn blocking(self) -> Vec<R::Output> {
        if matches!(self, Self::Init(_)) {
            actix_rt::Runtime::new()
                .expect("unable to create runtime")
                .block_on(self)
        } else {
            panic!("already been polled halfway")
        }
    }
}

impl<R: RequestTrait> Future for MsgRequestVec<R> {
    type Output = Vec<R::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MsgRequestVecProj::Init(reqs) => {
                let reqs = mem::take(reqs);
                let futs = futures::future::join_all(reqs);
                self.set(Self::Fut(futs));
                cx.waker().wake_by_ref(); // can be polled immediately to receive result
                Poll::Pending
            }
            MsgRequestVecProj::Fut(fut) => {
                let res = ready!(fut.poll(cx));
                self.set(Self::Gone); // seal this MsgRequestVec
                Poll::Ready(res)
            }
            MsgRequestVecProj::Gone => panic!("requests done and can't be polled again"),
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

    use crate::request::MsgRequestTuple;

    use super::{MsgRequest, MsgRequestVec, RequestTrait};

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
    async fn must_req_vec_do() {
        let addr1 = Echo::default().start();
        let addr2 = Echo::default().start();
        let addr3 = Echo::default().start();
        let reqs = MsgRequestVec::new([
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
    async fn must_req_vec_await() {
        let addr = Echo::default().start();
        let reqs = MsgRequestVec::new([
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
    fn must_req_vec_blocking() {
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
        let reqs = MsgRequestVec::new([MsgRequest::new(&addr, Ping(42, true))]);
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
    async fn must_req_tuple_do() {
        let addr1 = Echo::default().start();
        let addr2 = Echo::default().start();
        let reqs = MsgRequestTuple::new(
            MsgRequest::new(&addr1, Ping(41, false)),
            MsgRequest::new(&addr2, Ping(42, false)),
        );
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
    }

    #[actix::test]
    async fn must_req_tuple_await() {
        let addr = Echo::default().start();
        let reqs = MsgRequestTuple::new(
            MsgRequest::new(&addr, Ping(41, false)),
            MsgRequest::new(&addr, Ping(42, false)),
        );
        let res = reqs.await;
        assert_eq!(reqs.0.unwrap(), 41, "response incorrect");
        assert_eq!(reqs.1.unwrap(), 41, "response incorrect");
    }
}
