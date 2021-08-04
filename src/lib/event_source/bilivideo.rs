use actix::prelude::*;

use crate::error::Result;
use crate::{Update, Vtuber};

pub struct BiliVideoActor {}

impl BiliVideoActor {
    pub fn new() {}
}

impl Actor for BiliVideoActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {}
}

impl Handler<Update<Vtuber>> for BiliVideoActor {
    type Result = Result<()>;

    fn handle(&mut self, msg: Update<Vtuber>, ctx: &mut Self::Context) -> Self::Result {
        todo!()
    }
}
