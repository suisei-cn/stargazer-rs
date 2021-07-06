use actix::prelude::*;

use crate::lib::{Update, Vtuber};

pub struct TwitterActor {}

impl TwitterActor {
    pub fn new() {}
}

impl Actor for TwitterActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {}
}

impl Handler<Update<Vtuber>> for TwitterActor {
    fn handle(&mut self, msg: Update<Vtuber>, ctx: &mut Self::Context) -> Self::Result {}
}
