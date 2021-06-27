use actix::{Actor, Context, Handler};
use anyhow::Result;
use serde::Serialize;

use super::Event;

pub struct WsActor {}

impl WsActor {}

impl Actor for WsActor {
    type Context = Context<Self>;
}

impl<T: Serialize + Send> Handler<Event<T>> for WsActor {
    type Result = Result<()>;

    fn handle(&mut self, msg: Event<T>, ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
