use std::fmt::Debug;

use actix::{Actor, Context};
use serde::{de::DeserializeOwned, Serialize};

pub use actor::ScheduleActor;
pub use models::TaskInfo;

use crate::db::{Collection, Document};
use crate::utils::Scheduler;

pub mod actor;
mod config;
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

pub trait TaskFieldGetter: SchedulerGetter {}

impl<T> TaskFieldGetter for T where T: SchedulerGetter {}

pub trait Task: TaskFieldGetter + Actor<Context = Context<Self>> + Debug {
    type Entry: Debug + Serialize + DeserializeOwned + Send + Sync;
    type Ctor;
    fn query() -> Document;
    fn construct(
        entry: Self::Entry,
        ctor: Self::Ctor,
        scheduler: Scheduler<Self>,
        info: TaskInfo,
        collection: Collection,
    ) -> Self;
}
