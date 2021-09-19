use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use actix::{Actor, Addr, Context};
use serde::{de::DeserializeOwned, Serialize};

use models::TaskInfo;
use ops::UpdateTSOp;

use crate::db::{Collection, DBOperation, DBResult, Document};
use actor::ScheduleActor;

pub mod actor;
mod config;
pub mod messages;
mod models;
mod ops;
#[cfg(test)]
mod tests;

pub trait TaskInfoGetter {
    fn get_info(&self) -> TaskInfo;
    fn get_info_mut(&mut self) -> &mut TaskInfo;
}

pub trait CollectionGetter {
    fn get_collection(&self) -> &Collection;
}

pub trait SchedulerGetter {
    fn get_scheduler(&self) -> &Addr<ScheduleActor<Self>> where Self: Task;
}

pub trait TaskFieldGetter: TaskInfoGetter + CollectionGetter + SchedulerGetter {}

impl<T> TaskFieldGetter for T where T: TaskInfoGetter + CollectionGetter + SchedulerGetter{}

pub trait Task: TaskFieldGetter + Actor<Context=Context<Self>> + Debug {
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

    // Update the timestamp.
    // Returns `false` if the resource bound to this worker is replaced by another worker or deleted
    fn update_timestamp(&self) -> Pin<Box<dyn Future<Output = DBResult<bool>>>> {
        let collection = self.get_collection().clone();
        let task_info = self.get_info();
        let op = UpdateTSOp::new(task_info);
        Box::pin(async move { op.execute(&collection).await })
    }
}
