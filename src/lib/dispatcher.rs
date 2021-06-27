use actix::{Actor, Context, Handler};
use anyhow::Result;
use serde_json::Value;

use super::Event;

pub struct Dispatcher {}

impl Dispatcher {
    pub fn new() -> Self {
        Dispatcher {}
    }
}

impl Actor for Dispatcher {
    type Context = Context<Self>;
}

impl Handler<Event<Value>> for Dispatcher {
    type Result = Result<()>;

    fn handle(&mut self, msg: Event<Value>, ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}

impl Handler<Event<String>> for Dispatcher {
    type Result = Result<()>;

    fn handle(&mut self, msg: Event<String>, ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}

#[test]
fn test() {
    let dis = Dispatcher {}.start();
    let res = dis.recipient::<Event<String>>();
}
