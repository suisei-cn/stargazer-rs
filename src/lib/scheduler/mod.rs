use std::fmt::Debug;

use actix::{Actor, Context};
use actix_signal::SignalHandler;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::Span;

pub use actor::ScheduleActor;
pub use models::TaskInfo;

use crate::db::{Collection, DBRef, Document};
use crate::utils::Scheduler;

pub mod actor;
mod builder;
pub mod driver;
pub mod messages;
mod models;
mod ops;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct Entry<T> {
    pub root: DBRef,
    #[serde(flatten, bound(deserialize = "T: Deserialize<'de>"))]
    pub data: T,
}

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

pub trait Task: TaskFieldGetter + Actor<Context = Context<Self>> + SignalHandler + Debug {
    const COLLECTION_NAME: &'static str;
    type Entry: Debug + Serialize + DeserializeOwned + Send + Sync;
    type Ctor;
    fn query() -> Document;
    fn construct(
        entry: Entry<Self::Entry>,
        ctor: Self::Ctor,
        scheduler: Scheduler<Self>,
        info: TaskInfo,
        collection: Collection<Document>,
    ) -> Self;
    fn span(&self) -> Span;
}
