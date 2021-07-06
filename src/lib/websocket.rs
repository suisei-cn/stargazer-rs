use actix::{Actor, Context, Handler, StreamHandler, Supervised, SystemService};
use actix_web_actors::ws;
use log::{debug, info, warn};
use serde::Serialize;

use super::Event;

pub struct Websocket {}

impl Websocket {}

impl Actor for Websocket {
    type Context = ws::WebsocketContext<Self>;
}

impl Supervised for Websocket {
    fn restarting(&mut self, ctx: &mut Context<Self>) {
        warn!("Websocket service restarting");
    }
}

impl<T: Serialize, Deserialize, Send> Handler<Event<T>> for Websocket {
    fn handle(&mut self, msg: Event<T>, ctx: &mut Self::Context) -> Self::Result {
        ctx.text(msg.json()?);
        Ok(())
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Websocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => debug!("Received msg: {}", text),
            _ => (),
        }
    }
}
