use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use actix::{
    Actor, ActorFutureExt, Addr, AsyncContext, AtomicResponse, Context, Handler, ResponseActFuture,
    ResponseFuture, WrapFuture,
};
use serde::Serialize;
use tracing::{info, info_span, warn};
use tracing_actix::ActorInstrument;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::common::ResponseWrapper;
use crate::config::ScheduleConfig;
use crate::db::{Collection, DBOperation, DBResult, Document};
use crate::scheduler::driver::{RegisterScheduler, ScheduleDriverActor};
use crate::scheduler::messages::UpdateEntry;
use crate::scheduler::ops::{ScheduleMode, UpdateEntryOp};
use crate::scheduler::TaskInfo;

use super::messages::{ActorsIter, GetId, TriggerGC, TrySchedule};
use super::models::SchedulerMeta;
use super::ops::ScheduleOp;
use super::Task;

#[derive(Debug, Clone)]
pub struct ScheduleContext<T: Actor> {
    id: Uuid,
    actors: HashMap<TaskInfo, Addr<T>>,
}

impl<T: Actor> ScheduleContext<T> {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn actors(&self) -> &HashMap<TaskInfo, Addr<T>> {
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
    pub(crate) driver: Addr<ScheduleDriverActor<T>>,
}

impl_message_target!(pub ScheduleTarget, ScheduleActor<T: Task>);

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
    type Result = AtomicResponse<Self, DBResult<Option<(TaskInfo, Addr<T>)>>>;

    fn handle(&mut self, _msg: TrySchedule<T>, ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let config = self.config;
        let ctor_builder = self.ctor_builder.clone();

        let ctx_meta = SchedulerMeta::from(&self.ctx);
        let scheduler_addr = ctx.address();

        let scheduler_id = self.ctx.id;

        AtomicResponse::new(Box::pin(
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
                        info!("entry stolen: {:?}", entry);
                        let actor = T::construct(
                            entry,
                            (*ctor_builder)(),
                            scheduler_addr,
                            info,
                            collection,
                        );
                        let addr = actor.start();
                        (info, addr)
                    })
                })
            }
            .into_actor(self)
            .map(|resp, act, _ctx| {
                resp.map(|maybe_res| {
                    maybe_res.map(|(info, addr)| {
                        act.ctx.actors.insert(info, addr.clone());
                        (info, addr)
                    })
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
            .filter_map(|(info, addr)| (!addr.connected()).then(|| *info))
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|info| {
                warn!("Removing {} from actors", info.uuid());
                self.ctx.actors.remove(&info);
            });
    }
}

impl<A, F, Output> Handler<ActorsIter<A, F, Output>> for ScheduleActor<A>
where
    A: 'static + Task + Actor<Context = Context<A>> + Unpin,
    F: 'static + FnOnce(HashMap<TaskInfo, Addr<A>>) -> ResponseFuture<Output>,
    Output: 'static,
{
    type Result = ResponseActFuture<Self, Output>;

    fn handle(&mut self, msg: ActorsIter<A, F, Output>, ctx: &mut Self::Context) -> Self::Result {
        Box::pin(
            ctx.address()
                .send(TriggerGC)
                .into_actor(self)
                .then(|_, act, _ctx| (msg.into_inner())(act.ctx.actors.clone()).into_actor(act)),
        )
    }
}

impl<T> Actor for ScheduleActor<T>
where
    T: 'static + Task + Actor<Context = Context<T>> + Unpin,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.driver.do_send(RegisterScheduler::new(ctx.address()));
        ctx.run_interval(self.config.max_interval() / 2, |_, ctx| {
            ctx.notify(TriggerGC);
        });
    }
}
