use std::fmt::Debug;

use actix::{Actor, ActorFutureExt, Context, Message, ResponseActFuture, WrapFuture};
use serde::{de::DeserializeOwned, Serialize};
use tracing::Span;

pub use actor::ScheduleActor;
pub use models::TaskInfo;

use crate::db::{Collection, Document};
use crate::scheduler::messages::UpdateTimestamp;
use crate::utils::Scheduler;

pub mod actor;
pub mod messages;
mod models;
mod ops;
#[cfg(test)]
mod tests;

pub trait SchedulerGetter {
    fn get_scheduler(&self) -> &Scheduler<Self>
    where
        Self: Task;
}

pub trait InfoGetter {
    fn get_info(&self) -> TaskInfo;
}

pub trait TaskFieldGetter: SchedulerGetter + InfoGetter {}

impl<T> TaskFieldGetter for T where T: SchedulerGetter + InfoGetter {}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("bool")]
pub struct Tick;

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct TickOrStop;

pub trait Task: TaskFieldGetter + Actor<Context = Context<Self>> + Debug {
    type Entry: Debug + Serialize + DeserializeOwned + Send + Sync;
    type Ctor;
    fn query() -> Document;
    fn construct(
        entry: Self::Entry,
        ctor: Self::Ctor,
        scheduler: Scheduler<Self>,
        info: TaskInfo,
        collection: Collection<Document>,
    ) -> Self;
    fn span(&self) -> Span;
}

pub(crate) fn handle_tick<S: Task>(this: &S) -> ResponseActFuture<S, bool> {
    Box::pin(
        this.get_scheduler()
            .send(UpdateTimestamp(this.get_info()))
            .into_actor(this)
            .map(|res, _, _| res.unwrap_or(Ok(false)).unwrap_or(false)),
    )
}
