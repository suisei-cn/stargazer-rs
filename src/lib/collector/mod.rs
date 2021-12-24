use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Deref};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix::fut::{ready, wrap_future};
use actix::{
    Actor, ActorFutureExt, AsyncContext, AtomicResponse, Handler, Message, Recipient,
    ResponseActFuture, WrapFuture,
};
use arraydeque::{ArrayDeque, Wrapping};
use async_trait::async_trait;
use mongodb::Database;
use serde::Serialize;
use tracing::{debug, error, info, info_span, trace, warn, Span};
use tracing_actix::ActorInstrument;

use crate::db::{DBOperation, DBRef, DBResult};
use crate::manager::Vtuber;
use crate::ArbiterContext;

pub mod amqp;
pub mod debug;

#[cfg(test)]
mod tests;

fn span() -> Span {
    let arb_id = ArbiterContext::with(ArbiterContext::arbiter_id);
    info_span!("collector", arb=?arb_id)
}

// TODO split msg sent to dispatcher & collector impl
#[derive(Clone, Message)]
#[rtype("()")]
pub struct Publish {
    root: DBRef,
    topic: String,
    data: Arc<dyn erased_serde::Serialize + Send + Sync>,
}

impl Debug for Publish {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publish")
            .field("root", &self.root)
            .field("topic", &self.topic)
            .field("data", &"...")
            .finish()
    }
}

impl Publish {
    async fn expand(self, db: Database) -> DBResult<Option<PublishExpanded>> {
        let vtuber = self.root.get::<Vtuber>().execute(&db).await?;
        Ok(vtuber.map(|vtuber| PublishExpanded {
            vtuber: vtuber.name,
            topic: self.topic,
            data: self.data,
        }))
    }
}

#[derive(Clone, Message)]
#[rtype("bool")]
pub struct PublishExpanded {
    // TODO more fields?
    vtuber: String,
    topic: String,
    data: Arc<dyn erased_serde::Serialize + Send + Sync>,
}

impl Debug for PublishExpanded {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishExpanded")
            .field("vtuber", &self.vtuber)
            .field("topic", &self.topic)
            .field("data", &"...")
            .finish()
    }
}

impl Publish {
    /// Creates a new `Publish` event.
    ///
    /// # Panics
    /// Panics when given `data` can't be serialized into json.
    pub fn new<T: 'static + Serialize + Send + Sync>(root: DBRef, topic: &str, data: T) -> Self {
        Self {
            root,
            topic: topic.to_string(),
            data: Arc::new(data),
        }
    }
}

#[derive(Debug, Clone, Message)]
#[rtype("()")]
struct Wake(CollectorFactoryWrapped);

pub trait Collector: Actor<Context = actix::Context<Self>> + Handler<PublishExpanded> {}

#[async_trait]
pub trait CollectorFactory: Debug {
    fn ident(&self) -> String;
    async fn build(&self) -> Option<Recipient<PublishExpanded>>;
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
    Available(Recipient<PublishExpanded>),
    DelayedEstablish(Instant),
    Uninit,
}

impl Default for State {
    fn default() -> Self {
        Self::Uninit
    }
}

#[derive(Debug, Default)]
struct Context {
    state: State,
    queue: ArrayDeque<[PublishExpanded; 1024], Wrapping>,
}

pub struct CollectorActor {
    db: Database,
    collectors: HashMap<CollectorFactoryWrapped, Context>,
}

impl_stop_on_panic!(CollectorActor);

impl CollectorActor {
    pub fn new(db: Database, factories: Vec<CollectorFactoryWrapped>) -> Self {
        Self {
            db,
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
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: Publish, _: &mut Self::Context) -> Self::Result {
        msg.expand(self.db.clone())
            .into_actor(self)
            .map(|msg, act, ctx| {
                if let Ok(Some(msg)) = msg {
                    act.collectors
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
                } else {
                    span().in_scope(|| warn!("unable to fetch root metadata"));
                }
            })
            .boxed_local()
    }
}

impl Handler<Wake> for CollectorActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, msg: Wake, ctx: &mut Self::Context) -> Self::Result {
        enum Branch<'a> {
            Send(&'a mut Recipient<PublishExpanded>),
            EstablishConnection,
            EarlyWake(Instant),
        }
        span().in_scope(|| trace!("waked"));
        self.collectors.get_mut(&msg.0).map_or_else(
            || {
                span().in_scope(|| error!("collector not found"));
                AtomicResponse::new(Box::pin(ready(())))
            },
            |collector_ctx| {
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
                        event.map_or_else(
                            || {
                                span().in_scope(|| {
                                    warn!("wake but no event available. this might be a bug");
                                });
                                // no event available
                                AtomicResponse::new(Box::pin(ready(())))
                            },
                            |event| {
                                span().in_scope(|| debug!("dispatching event"));
                                AtomicResponse::new(Box::pin(
                                    wrap_future::<_, Self>(collector.send(event.clone()))
                                        .map(move |succ, act, ctx| {
                                            if let Some(collector_ctx) =
                                                act.collectors.get_mut(&msg.0)
                                            {
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
                            },
                        )
                    }
                    Branch::EstablishConnection => {
                        // deadline reached (or uninit), may retry
                        let factory = msg.0.clone();
                        AtomicResponse::new(Box::pin(
                            wrap_future::<_, Self>(async move {
                                debug!("establishing connection");
                                factory.build().await
                            })
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
            },
        )
    }
}
