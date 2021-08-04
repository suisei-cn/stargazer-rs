use actix::prelude::*;

use crate::error::Result;
use crate::{Update, Vtuber};

pub struct BiliLiveActor {
    vtuber: Vec<Vtuber>,
}

impl BiliLiveActor {
    pub fn new() {}
}

pub struct BiliLiveRoomActor {}

impl BiliLiveRoomActor {
    pub fn new() {}
}

impl Actor for BiliLiveRoomActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {}
}

impl Handler<Update<Vtuber>> for BiliLiveRoomActor {
    type Result = Result<()>;

    fn handle(&mut self, msg: Update<Vtuber>, ctx: &mut Self::Context) -> Self::Result {
        todo!()
    }
}
