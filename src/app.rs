use actix::prelude::*;

pub struct App {}

/// App is the ultimate instance to interact with
/// It should be responsible for setting up all the actors
/// Including EventEmitter, MiddleWare and Sender
impl App {
    pub fn new() -> Self {
        App {}
    }

    pub async fn serve(&mut self) {
        loop {}
    }
}
