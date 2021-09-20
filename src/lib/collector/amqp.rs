use actix::{
    Actor, ActorContext, ActorFutureExt, Context, Handler, Message, ResponseActFuture, WrapFuture,
};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind, Result};
use serde::Serialize;
use tokio_amqp::LapinTokioExt;
use tracing::{error, info_span};
use tracing_actix::ActorInstrument;

#[derive(Debug, Clone, Message)]
#[rtype("Result<()>")]
pub struct Publish<T> {
    topic: String,
    data: T,
}

impl<T> Publish<T> {
    pub const fn new(topic: String, data: T) -> Self {
        Self { topic, data }
    }
}

#[derive(Debug, Clone)]
pub struct AMQPActor {
    channel: Channel,
    exchange: String,
}

impl AMQPActor {
    pub async fn new(uri: &str, exchange: &str) -> Result<Self> {
        let conn = Connection::connect(uri, ConnectionProperties::default().with_tokio()).await?;
        let channel = conn.create_channel().await?;
        channel
            .exchange_declare(
                exchange,
                ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                Default::default(),
            )
            .await?;
        Ok(Self {
            channel,
            exchange: exchange.to_string(),
        })
    }
}

impl Actor for AMQPActor {
    type Context = Context<Self>;
}

impl<T: Serialize> Handler<Publish<T>> for AMQPActor {
    type Result = ResponseActFuture<Self, Result<()>>;

    fn handle(&mut self, msg: Publish<T>, _ctx: &mut Self::Context) -> Self::Result {
        let payload = serde_json::to_vec(&msg.data).unwrap(); // TODO error handling
        Box::pin(
            self.channel
                .basic_publish(
                    self.exchange.as_str(),
                    msg.topic.as_str(),
                    BasicPublishOptions::default(),
                    payload,
                    BasicProperties::default(),
                )
                .into_actor(self)
                .map(|res, _act, ctx| match res {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        error!("publish error: {:?}, stopping actor", e);
                        ctx.stop();
                        Err(e)
                    }
                })
                .actor_instrument(info_span!("amqp")),
        )
    }
}
