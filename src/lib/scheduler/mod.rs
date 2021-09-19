use std::fmt::Debug;

use actix::{Actor, Addr, Context};
use serde::{de::DeserializeOwned, Serialize};

use actor::ScheduleActor;
use models::TaskInfo;

use crate::db::{Collection, Document};

pub mod actor;
mod config;
pub mod messages;
mod models;
mod ops;
#[cfg(test)]
mod tests;

pub trait SchedulerGetter {
    fn get_scheduler(&self) -> &Addr<ScheduleActor<Self>>
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
        scheduler: Addr<ScheduleActor<Self>>,
        info: TaskInfo,
        collection: Collection,
    ) -> Self;
}
