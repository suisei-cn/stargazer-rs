use std::fmt::Debug;

use actix::Actor;
use mongodb::{bson::Document, Collection};
use serde::{de::DeserializeOwned, Serialize};

use crate::scheduler::models::TaskInfo;

mod models;
mod ops;
#[cfg(test)]
mod tests;

pub trait TaskInfoGetter {
    fn get_info(&self) -> TaskInfo;
    fn get_info_mut(&mut self) -> &mut TaskInfo;
}

pub trait CollectionGetter {
    fn get_collection(&self) -> &Collection<Document>;
}

pub trait TaskFieldGetter: TaskInfoGetter + CollectionGetter {}

impl<T> TaskFieldGetter for T where T: TaskInfoGetter + CollectionGetter {}

pub trait Task: TaskFieldGetter + Actor + Debug {
    type Entry: Debug + Serialize + DeserializeOwned + Send + Sync;
    type Ctor;
    fn query() -> Document;
    fn construct(
        entry: Self::Entry,
        ctor: Self::Ctor,
        info: TaskInfo,
        collection: Collection<Document>,
    ) -> Self;

    // Update the timestamp.
    // Returns `false` if the resource bound to this worker is replaced by another worker or deleted
    // fn update_timestamp(&self) -> Pin<Box<dyn Future<Output = DBResult<bool>>>> {
    //     let collection = self.get_collection().clone();
    //     let steal_info = self.get_info().clone();
    //     let op = UpdateTSOp::new(steal_info);
    //     Box::pin(async move { op.execute(&collection).await })
    // }
}
