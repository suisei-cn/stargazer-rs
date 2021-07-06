use crate::lib::Websocket;
use actix::prelude::*;

pub struct App {
    ws: Addr<Websocket>,
}

/// App is the ultimate instance to interact with
/// It should be responsible for setting up all the actors
/// Including EventEmitter, MiddleWare and Sender
impl App {
    pub fn new() -> Self {
        let ws = Websocket::start_default();
        App {}
    }

    pub async fn serve(&mut self) {
        loop {}
    }
}
