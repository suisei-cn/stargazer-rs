use std::fmt::Debug;

use actix::{
    Actor, ActorContext, ActorFutureExt, Context, Handler, Recipient, ResponseActFuture, WrapFuture,
};
use async_trait::async_trait;
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind, Result};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tokio_amqp::LapinTokioExt;
use tracing::{error, info_span, Instrument, Span};
use tracing_actix::ActorInstrument;

use crate::collector::{Collector, CollectorFactory};

use super::Publish;

static AMQP_CONNECTION: Lazy<Mutex<Option<Connection>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug)]
pub struct AMQPFactory {
    uri: String,
    exchange: String,
}

impl AMQPFactory {
    pub fn new(uri: &str, exchange: &str) -> Self {
        Self {
            uri: uri.to_string(),
            exchange: exchange.to_string(),
        }
    }
    fn span(&self) -> Span {
        info_span!("amqp_factory", uri = %self.uri, exchange = %self.exchange)
    }
}

#[async_trait]
impl CollectorFactory for AMQPFactory {
    fn ident(&self) -> String {
        format!("AMQP(uri={}, exchange={})", self.uri, self.exchange)
    }

    async fn build(&self) -> Option<Recipient<Publish>> {
        async fn _build(uri: &str, exchange: &str, update_conn: bool) -> Result<AMQPActor> {
            let mut guard = AMQP_CONNECTION.lock().await;
            if guard.is_none() || update_conn {
                *guard = Some(
                    Connection::connect(uri, ConnectionProperties::default().with_tokio()).await?,
                );
            }
            let conn = guard.as_ref().unwrap();
            let chan = conn.create_channel().await?;
            AMQPActor::new(chan, uri, exchange).await
        }
        match _build(self.uri.as_str(), self.exchange.as_str(), false)
            .instrument(self.span())
            .await
        {
            Ok(act) => Some(act.start().recipient()),
            Err(_) => match _build(self.uri.as_str(), self.exchange.as_str(), true)
                .instrument(self.span())
                .await
            {
                Ok(act) => Some(act.start().recipient()),
                Err(e) => {
                    self.span()
                        .in_scope(|| error!("amqp connect fail: {:?}", e));
                    None
                }
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct AMQPActor {
    channel: Channel,
    uri: String,
    exchange: String,
}

impl_stop_on_panic!(AMQPActor);

impl Collector for AMQPActor {}

impl AMQPActor {
    /// Creates a new `AMQPActor` with given channel and exchange.
    /// The `uri` parameter is for logging usage.
    ///
    /// # Errors
    /// Raise errors if unable to declare exchange on current channel.
    pub async fn new(channel: Channel, uri: &str, exchange: &str) -> Result<Self> {
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
            uri: uri.to_string(),
            exchange: exchange.to_string(),
        })
    }

    fn span(&self) -> Span {
        info_span!("amqp", uri=%self.uri, exchange=%self.exchange)
    }
}

impl Actor for AMQPActor {
    type Context = Context<Self>;
}

impl Handler<Publish> for AMQPActor {
    type Result = ResponseActFuture<Self, bool>;

    fn handle(&mut self, msg: Publish, _ctx: &mut Self::Context) -> Self::Result {
        let payload = serde_json::to_vec(&*msg.data).unwrap(); // TODO error handling
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
                    Ok(_) => true,
                    Err(e) => {
                        error!("publish error: {:?}, stopping actor", e);
                        ctx.stop();
                        false
                    }
                })
                .actor_instrument(self.span()),
        )
    }
}
