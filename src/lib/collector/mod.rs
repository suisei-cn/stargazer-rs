use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::ops::{Add, Deref};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix::fut::ready;
use actix::{
    Actor, ActorFutureExt, AsyncContext, AtomicResponse, Handler, Message, Recipient, WrapFuture,
};
use arraydeque::{ArrayDeque, Wrapping};
use async_trait::async_trait;
use serde::Serialize;
use tracing::{debug, error, info, info_span, warn, Span};
use tracing_actix::ActorInstrument;

use crate::ArbiterContext;

pub mod amqp;

fn span() -> Span {
    let arb_id = ArbiterContext::with(|ctx| ctx.arbiter_id());
    info_span!("collector", arb=?arb_id)
}

#[derive(Debug, Clone, Message)]
#[rtype("bool")]
pub struct Publish {
    topic: String,
    data: Arc<serde_json::Value>,
}

impl Publish {
    pub fn new<T: Serialize>(topic: String, data: T) -> Self {
        Self {
            topic,
            data: Arc::new(serde_json::to_value(data).unwrap()),
        }
    }
}

#[derive(Debug, Clone, Message)]
#[rtype("()")]
struct Wake(CollectorFactoryWrapped);

pub trait Collector: Actor<Context = actix::Context<Self>> + Handler<Publish> {}

#[async_trait]
pub trait CollectorFactory: Debug {
    fn ident(&self) -> String;
    async fn build(&self) -> Option<Recipient<Publish>>;
}

#[derive(Debug, Clone)]
pub struct CollectorFactoryWrapped(Rc<dyn CollectorFactory>);

impl<T: 'static + CollectorFactory> From<T> for CollectorFactoryWrapped {
    fn from(factory: T) -> Self {
        Self(Rc::new(factory))
    }
}

impl Hash for CollectorFactoryWrapped {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.ident().hash(state);
    }
}

impl PartialEq for CollectorFactoryWrapped {
    fn eq(&self, other: &Self) -> bool {
        self.0.ident() == other.0.ident()
    }
}

impl Eq for CollectorFactoryWrapped {}

impl Deref for CollectorFactoryWrapped {
    type Target = Rc<dyn CollectorFactory>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
enum State {
    Available(Recipient<Publish>),
    DelayedEstablish(Instant),
    Uninit,
}

impl Default for State {
    fn default() -> Self {
        State::Uninit
    }
}

#[derive(Debug, Default)]
struct Context {
    state: State,
    queue: ArrayDeque<[Publish; 1024], Wrapping>,
}

pub struct CollectorActor {
    collectors: HashMap<CollectorFactoryWrapped, Context>,
}

impl_stop_on_panic!(CollectorActor);
impl_message_target!(pub CollectorTarget, CollectorActor);

impl CollectorActor {
    pub fn new(factories: Vec<CollectorFactoryWrapped>) -> Self {
        Self {
            collectors: factories
                .into_iter()
                .map(|factory| (factory, Context::default()))
                .collect(),
        }
    }
}

impl Actor for CollectorActor {
    type Context = actix::Context<Self>;
}

impl Handler<Publish> for CollectorActor {
    type Result = bool;

    fn handle(&mut self, msg: Publish, ctx: &mut Self::Context) -> Self::Result {
        self.collectors
            .iter_mut()
            .for_each(|(factory, collector_ctx)| {
                if matches!(collector_ctx.state, State::Available(_))
                    && collector_ctx.queue.is_empty()
                    || matches!(collector_ctx.state, State::Uninit)
                {
                    // collector available & queue empty | lazy init, schedule wake
                    ctx.notify(Wake(factory.clone()));
                }
                collector_ctx.queue.push_back(msg.clone());
            });
        true
    }
}

impl Handler<Wake> for CollectorActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, msg: Wake, ctx: &mut Self::Context) -> Self::Result {
        enum Branch<'a> {
            Send(&'a mut Recipient<Publish>),
            EstablishConnection,
            EarlyWake(Instant),
        }
        span().in_scope(|| debug!("waked"));
        if let Some(collector_ctx) = self.collectors.get_mut(&msg.0) {
            let branch = match &mut collector_ctx.state {
                State::Available(recipient) => Branch::Send(recipient),
                State::DelayedEstablish(deadline) if Instant::now() >= *deadline => {
                    Branch::EstablishConnection
                }
                State::DelayedEstablish(deadline) => Branch::EarlyWake(*deadline),
                State::Uninit => Branch::EstablishConnection,
            };
            match branch {
                Branch::Send(collector) => {
                    let event = collector_ctx.queue.pop_front();
                    if let Some(event) = event {
                        span().in_scope(|| debug!("dispatching event"));
                        AtomicResponse::new(Box::pin(
                            collector
                                .send(event.clone())
                                .into_actor(self)
                                .map(move |succ, act, ctx| {
                                    if let Some(collector_ctx) = act.collectors.get_mut(&msg.0) {
                                        if succ.unwrap_or_default() {
                                            // event sent
                                            if !collector_ctx.queue.is_empty() {
                                                // there's event remaining in queue, schedule wake
                                                ctx.notify(msg);
                                            }
                                        } else {
                                            error!("failed to dispatch event");
                                            // failed to send event
                                            // update state
                                            collector_ctx.state = State::DelayedEstablish(
                                                Instant::now().add(Duration::from_secs(10)), // TODO config retry
                                            );
                                            // put back unsent event
                                            collector_ctx.queue.push_front(event);
                                            // schedule delayed wake
                                            ctx.notify_later(msg, Duration::from_secs(10));
                                            // TODO config retry
                                        }
                                    } else {
                                        error!("collector not found");
                                    }
                                })
                                .actor_instrument(span()),
                        ))
                    } else {
                        span()
                            .in_scope(|| warn!("wake but no event available. this might be a bug"));
                        // no event available
                        AtomicResponse::new(Box::pin(ready(())))
                    }
                }
                Branch::EstablishConnection => {
                    // deadline reached (or uninit), may retry
                    let factory = msg.0.clone();
                    AtomicResponse::new(Box::pin(
                        async move {
                            debug!("establishing connection");
                            factory.build().await
                        }
                        .into_actor(self)
                        .map(move |recipient, act, ctx| {
                            if let Some(collector_ctx) = act.collectors.get_mut(&msg.0) {
                                if let Some(recipient) = recipient {
                                    info!("connection established");
                                    // got new recipient
                                    collector_ctx.state = State::Available(recipient);
                                    if !collector_ctx.queue.is_empty() {
                                        // there's event remaining in queue, schedule wake
                                        ctx.notify(msg);
                                    }
                                } else {
                                    error!("failed to establish connection");
                                    // failed to build new recipient, schedule delayed wake
                                    collector_ctx.state = State::DelayedEstablish(
                                        Instant::now().add(Duration::from_secs(10)), // TODO config retry
                                    );
                                    ctx.notify_later(msg, Duration::from_secs(10));
                                    // TODO config retry
                                }
                            } else {
                                error!("collector not found");
                            }
                        })
                        .actor_instrument(span()),
                    ))
                }
                Branch::EarlyWake(deadline) => {
                    span().in_scope(|| warn!("early wake"));
                    // early wake
                    ctx.notify_later(msg, deadline.duration_since(Instant::now()));
                    AtomicResponse::new(Box::pin(ready(())))
                }
            }
        } else {
            span().in_scope(|| error!("collector not found"));
            AtomicResponse::new(Box::pin(ready(())))
        }
    }
}
