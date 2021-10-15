use actix::{Actor, Context, Handler, Message, System};
use derive_new::new;
use uuid::Uuid;

use crate::common::ResponseWrapper;
use crate::scheduler::messages::GetId;

#[derive(Debug, Copy, Clone, Message)]
#[rtype("usize")]
pub struct Ping(pub usize, pub bool);

#[derive(Debug, Message)]
#[rtype("usize")]
pub struct Query;

#[derive(Debug, Default)]
pub struct Echo(pub usize);

impl_message_target!(pub EchoTarget, Echo);

impl Actor for Echo {
    type Context = Context<Self>;
}

impl Handler<Ping> for Echo {
    type Result = usize;

    fn handle(&mut self, msg: Ping, _ctx: &mut Self::Context) -> Self::Result {
        self.0 = msg.0;
        if msg.1 {
            System::current().stop();
        }
        msg.0
    }
}

impl Handler<Query> for Echo {
    type Result = usize;

    fn handle(&mut self, _msg: Query, _ctx: &mut Self::Context) -> Self::Result {
        self.0
    }
}

impl Handler<GetId> for Echo {
    type Result = ResponseWrapper<Uuid>;

    fn handle(&mut self, _msg: GetId, _ctx: &mut Self::Context) -> Self::Result {
        ResponseWrapper(Uuid::nil())
    }
}

#[derive(Debug, Message)]
#[rtype("usize")]
pub struct Ping2(pub usize, pub bool);

#[derive(Debug, Default)]
pub struct Echo2(pub usize);

impl_message_target!(pub Echo2Target, Echo2);

impl Actor for Echo2 {
    type Context = Context<Self>;
}

impl Handler<Ping2> for Echo2 {
    type Result = usize;

    fn handle(&mut self, msg: Ping2, _ctx: &mut Self::Context) -> Self::Result {
        self.0 = msg.0;
        if msg.1 {
            System::current().stop();
        }
        msg.0
    }
}

impl Handler<Query> for Echo2 {
    type Result = usize;

    fn handle(&mut self, _msg: Query, _ctx: &mut Self::Context) -> Self::Result {
        self.0
    }
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("usize")]
pub struct Val(pub usize);

#[derive(Debug, new)]
pub struct Adder(pub usize);

impl_message_target!(pub AdderTarget, Adder);

impl Actor for Adder {
    type Context = Context<Self>;
}

impl Handler<Val> for Adder {
    type Result = usize;

    fn handle(&mut self, msg: Val, _ctx: &mut Self::Context) -> Self::Result {
        msg.0 + self.0
    }
}

impl Handler<GetId> for Adder {
    type Result = ResponseWrapper<Uuid>;

    fn handle(&mut self, _msg: GetId, _ctx: &mut Self::Context) -> Self::Result {
        ResponseWrapper(Uuid::from_u128(self.0 as u128))
    }
}

mod utils {
    use std::fmt::{Debug, Formatter};
    use std::marker::PhantomData;
    use std::rc::Rc;

    use actix::{Actor, Context};
    use actix_signal::SignalHandler;
    use tracing::Span;

    use crate::db::{Collection, Document};
    use crate::scheduler::{Task, TaskInfo};
    use crate::utils::Scheduler;

    pub trait StaticUnpinned: 'static + Unpin {}

    #[derive(SignalHandler)]
    pub struct A<T: StaticUnpinned, U: StaticUnpinned + Copy + Eq> {
        _marker_1: PhantomData<(T, U)>,
        _marker_2: Rc<()>,
        info: TaskInfo,
        scheduler: Scheduler<Self>,
    }

    impl<T: StaticUnpinned, U: StaticUnpinned + Copy + Eq> Debug for A<T, U> {
        fn fmt(&self, _: &mut Formatter<'_>) -> std::fmt::Result {
            unimplemented!()
        }
    }

    impl<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned> Actor for A<T, U> {
        type Context = Context<Self>;
    }

    impl<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned> Task for A<T, U> {
        type Entry = ();
        type Ctor = ();

        fn query() -> Document {
            unimplemented!()
        }

        fn construct(
            _: Self::Entry,
            _: Self::Ctor,
            _: Scheduler<Self>,
            _: TaskInfo,
            _: Collection<Document>,
        ) -> Self {
            unimplemented!()
        }

        fn span(&self) -> Span {
            unimplemented!()
        }
    }

    impl_message_target!(ATarget, A<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned>);
    impl_message_target!(pub PubATarget, A<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned>);
    impl_stop_on_panic!(A<T: StaticUnpinned, U: StaticUnpinned+ Copy + Eq>);
    impl_to_collector_handler!(A<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned>);
    impl_task_field_getter!(A<T: StaticUnpinned, U: StaticUnpinned+ Copy + Eq>, info, scheduler);
}
