use actix::{Actor, Context, Handler, Message, System};
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

impl Handler<GetId> for Echo {
    type Result = ResponseWrapper<Uuid>;

    fn handle(&mut self, msg: GetId, ctx: &mut Self::Context) -> Self::Result {
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

    fn handle(&mut self, msg: Ping2, ctx: &mut Self::Context) -> Self::Result {
        self.0 = msg.0;
        if msg.1 {
            System::current().stop();
        }
        msg.0
    }
}

impl Handler<Query> for Echo2 {
    type Result = usize;

    fn handle(&mut self, msg: Query, ctx: &mut Self::Context) -> Self::Result {
        self.0
    }
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("usize")]
pub struct Val(pub usize);

#[derive(Debug)]
pub struct Adder(pub usize);

impl Adder {
    pub fn new(other: usize) -> Self {
        Self(other)
    }
}

impl_message_target!(pub AdderTarget, Adder);

impl Actor for Adder {
    type Context = Context<Self>;
}

impl Handler<Val> for Adder {
    type Result = usize;

    fn handle(&mut self, msg: Val, ctx: &mut Self::Context) -> Self::Result {
        msg.0 + self.0
    }
}

impl Handler<GetId> for Adder {
    type Result = ResponseWrapper<Uuid>;

    fn handle(&mut self, msg: GetId, ctx: &mut Self::Context) -> Self::Result {
        ResponseWrapper(Uuid::from_u128(self.0 as u128))
    }
}
