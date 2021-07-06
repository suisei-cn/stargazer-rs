use actix::prelude::*;

use crate::lib::{Update, Vtuber};

pub struct BiliVideoActor {}

impl BiliVideoActor {
    pub fn new() {}
}

impl Actor for BiliVideoActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {}
}

impl Handler<Update<Vtuber>> for BiliVideoActor {
    fn handle(&mut self, msg: Update<Vtuber>, ctx: &mut Self::Context) -> Self::Result {}
}
