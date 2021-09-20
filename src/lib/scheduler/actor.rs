use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use actix::{
    Actor, ActorFutureExt, Addr, AsyncContext, Context, Handler, ResponseActFuture, ResponseFuture,
    WrapFuture,
};
use tracing::{info, info_span, warn};
use tracing_actix::ActorInstrument;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::common::ResponseWrapper;
use crate::config::ScheduleConfig;
use crate::db::{Collection, DBOperation, DBResult, Document};
use crate::scheduler::messages::UpdateTimestamp;
use crate::scheduler::ops::UpdateTSOp;

use super::messages::{ActorsIter, GetId, TriggerGC, TrySchedule};
use super::models::SchedulerMeta;
use super::ops::{ScheduleMode, ScheduleOp};
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
    type Result = ResponseActFuture<Self, DBResult<Option<(Uuid, Addr<T>)>>>;

    fn handle(&mut self, _msg: TrySchedule<T>, ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let config = self.config;
        let ctor_builder = self.ctor_builder.clone();

        let ctx_meta = SchedulerMeta::from(&self.ctx);
        let scheduler_addr = ctx.address();

        let scheduler_id = self.ctx.id;

        Box::pin(
            async move {
                ScheduleOp::new(
                    ScheduleMode::Auto,
                    T::query(),
                    ctx_meta,
                    config.max_interval(),
                )
                .execute(&collection)
                .await
                .map(|maybe_res| {
                    maybe_res.map(|(info, entry)| {
                        // We've got an entry.
                        let uuid = info.uuid();
                        info!("entry stolen: {:?}", entry);
                        let actor = T::construct(
                            entry,
                            (*ctor_builder)(),
                            scheduler_addr,
                            info,
                            collection,
                        );
                        let addr = actor.start();
                        (uuid, addr)
                    })
                })
            }
            .into_actor(self)
            .map(|resp, act, _ctx| {
                resp.map(|maybe_res| {
                    maybe_res.map(|(uuid, addr)| {
                        act.ctx.actors.insert(uuid, addr.clone());
                        (uuid, addr)
                    })
                })
            })
            .actor_instrument(info_span!("scheduler", id=?scheduler_id)),
        )
    }
}

impl<T> Handler<UpdateTimestamp> for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
{
    type Result = ResponseFuture<DBResult<bool>>;

    fn handle(&mut self, msg: UpdateTimestamp, _ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let op = UpdateTSOp::new(msg.0);
        Box::pin(async move { op.execute(&collection).await })
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
        // schedule task
        ctx.run_interval(self.config.schedule_interval(), |_, ctx| {
            ctx.notify(TrySchedule::new());
        });

        // gc task
        ctx.run_interval(self.config.max_interval() / 2, |_, ctx| {
            ctx.notify(TriggerGC);
        });
    }
}
