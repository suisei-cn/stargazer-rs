use actix::prelude::*;

use crate::lib::{Update, Vtuber};

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
    fn handle(&mut self, msg: Update<Vtuber>, ctx: &mut Self::Context) -> Self::Result {}
}
