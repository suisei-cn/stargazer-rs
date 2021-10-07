use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use actix::{
    Actor, ActorFutureExt, Addr, AsyncContext, AtomicResponse, Context, Handler, ResponseFuture,
    WrapFuture,
};
use itertools::Itertools;
use rand::{thread_rng, Rng};
use serde::Serialize;
use tracing::{info, info_span, warn};
use tracing_actix::ActorInstrument;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::common::ResponseWrapper;
use crate::config::ScheduleConfig;
use crate::context::MessageTarget;
use crate::db::{Collection, DBOperation, DBResult, Document};
use crate::scheduler::messages::UpdateEntry;
use crate::scheduler::ops::{ScheduleMode, UpdateEntryOp};

use super::messages::{ActorsIter, GetId, TriggerGC, TrySchedule};
use super::models::SchedulerMeta;
use super::ops::ScheduleOp;
use super::Task;

#[derive(Debug, Clone)]
pub struct ScheduleContext<T: Actor> {
    id: Uuid,
    actors: HashMap<Uuid, Addr<T>>,
}

impl<T: Actor> ScheduleContext<T> {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn actors(&self) -> &HashMap<Uuid, Addr<T>> {
        &self.actors
    }
}

impl<T: Actor> Default for ScheduleContext<T> {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            actors: HashMap::new(),
        }
    }
}

#[derive(Clone, TypedBuilder)]
pub struct ScheduleActor<T>
where
    T: Task,
{
    pub(crate) collection: Collection<Document>,
    #[builder(setter(transform = | f: impl Fn() -> T::Ctor + Send + Sync + 'static | Arc::new(f) as Arc < dyn Fn() -> T::Ctor + Send + Sync >))]
    pub(crate) ctor_builder: Arc<dyn Fn() -> T::Ctor + Send + Sync>,
    pub(crate) config: ScheduleConfig,
    #[builder(default, setter(skip))]
    pub(crate) ctx: ScheduleContext<T>,
}

#[derive(Debug)]
pub struct ScheduleTarget<T>(PhantomData<T>);

impl<T> ScheduleTarget<T> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<T> Default for ScheduleTarget<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for ScheduleTarget<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}
impl<T> Copy for ScheduleTarget<T> {}

unsafe impl<T> Send for ScheduleTarget<T> {}

impl<T: Task> MessageTarget for ScheduleTarget<T> {
    type Actor = ScheduleActor<T>;
    type Addr = Addr<ScheduleActor<T>>;
}

impl<T: Task> Debug for ScheduleActor<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StealActor")
            .field("collection", &self.collection)
            .field("ctor_builder", &"<func>")
            .field("config", &self.config)
            .field("ctx", &self.ctx)
            .finish()
    }
}

impl<T> Handler<TrySchedule<T>> for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
{
    #[allow(clippy::type_complexity)]
    type Result = AtomicResponse<Self, DBResult<Vec<(Uuid, Addr<T>)>>>;

    fn handle(&mut self, msg: TrySchedule<T>, ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let config = self.config;
        let ctor_builder = self.ctor_builder.clone();

        let ctx_meta = SchedulerMeta::from(&self.ctx);
        let scheduler_addr = ctx.address();

        let scheduler_id = self.ctx.id;

        AtomicResponse::new(Box::pin(
            async move {
                ScheduleOp::new(msg.0, T::query(), ctx_meta, config.max_interval())
                    .execute(&collection)
                    .await
                    .map(|res| {
                        res.into_iter()
                            .map(|(info, entry)| {
                                // We've got an entry.
                                let uuid = info.uuid();
                                info!("entry stolen: {:?}", entry);
                                let actor = T::construct(
                                    entry,
                                    (*ctor_builder)(),
                                    scheduler_addr.clone(),
                                    info,
                                    collection.clone(),
                                );
                                let addr = actor.start();
                                (uuid, addr)
                            })
                            .collect_vec()
                    })
            }
            .into_actor(self)
            .map(|resp, act, _ctx| {
                resp.map(|res| {
                    res.into_iter()
                        .map(|(uuid, addr)| {
                            act.ctx.actors.insert(uuid, addr.clone());
                            (uuid, addr)
                        })
                        .collect_vec()
                })
            })
            .actor_instrument(info_span!("scheduler", id=?scheduler_id)),
        ))
    }
}

impl<T, U> Handler<UpdateEntry<U>> for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
    U: 'static + Serialize + Send,
{
    type Result = AtomicResponse<Self, DBResult<bool>>;

    fn handle(&mut self, msg: UpdateEntry<U>, _ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let op = UpdateEntryOp::new(msg.info, msg.body);
        AtomicResponse::new(Box::pin(
            async move { op.execute(&collection).await }.into_actor(self),
        ))
    }
}

impl<T> Handler<GetId> for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
{
    type Result = ResponseWrapper<Uuid>;

    fn handle(&mut self, _msg: GetId, _ctx: &mut Self::Context) -> Self::Result {
        ResponseWrapper(self.ctx.id)
    }
}

impl<T> Handler<TriggerGC> for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
{
    type Result = ();

    fn handle(&mut self, _msg: TriggerGC, _ctx: &mut Self::Context) -> Self::Result {
        let _span = info_span!("scheduler", id=?self.ctx.id).entered();
        self.ctx
            .actors
            .iter()
            .filter_map(|(uuid, addr)| (!addr.connected()).then(|| *uuid))
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|uuid| {
                warn!("Removing {} from actors", uuid);
                self.ctx.actors.remove(&uuid);
            });
    }
}

impl<A, F, Output> Handler<ActorsIter<A, F, Output>> for ScheduleActor<A>
where
    A: 'static + Task + Actor<Context = Context<A>> + Unpin,
    F: FnOnce(HashMap<Uuid, Addr<A>>) -> Pin<Box<dyn Future<Output = Output>>>,
    Output: 'static,
{
    type Result = ResponseFuture<Output>;

    fn handle(&mut self, msg: ActorsIter<A, F, Output>, _ctx: &mut Self::Context) -> Self::Result {
        (msg.into_inner())(self.ctx.actors.clone())
    }
}

impl<T> Actor for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut skip_once = Box::new(true);

        // schedule task
        ctx.run_interval(self.config.schedule_interval(), |_, ctx| {
            let delay: u64 = thread_rng().gen_range(0..1000);
            ctx.notify_later(
                TrySchedule::new(ScheduleMode::OutdatedOnly),
                Duration::from_millis(delay),
            );
        });
        ctx.run_interval(self.config.balance_interval(), move |_, ctx| {
            if *skip_once {
                *skip_once = false;
                return;
            }
            let delay: u64 = thread_rng().gen_range(0..1000);
            ctx.notify_later(
                TrySchedule::new(ScheduleMode::Auto),
                Duration::from_millis(delay),
            );
        });

        // gc task
        ctx.run_interval(self.config.max_interval() / 2, |_, ctx| {
            ctx.notify(TriggerGC);
        });
    }
}
