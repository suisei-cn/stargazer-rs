use actix::prelude::*;

use crate::lib::{Update, Vtuber};

pub struct YoutubeActor {}

impl YoutubeActor {
    pub fn new() {}
}

impl Actor for YoutubeActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {}
}

impl Handler<Update<Vtuber>> for YoutubeActor {
    fn handle(&mut self, msg: Update<Vtuber>, ctx: &mut Self::Context) -> Self::Result {}
}
