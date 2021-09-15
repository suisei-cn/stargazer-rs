use std::mem::MaybeUninit;

use mongodb::{bson::Document, Collection};

use crate::scheduler::TaskFieldGetter;

use super::models::TaskInfo;

#[derive(Debug, Clone)]
struct DummyTask {
    info: TaskInfo,
    collection: Collection<Document>,
}

impl_task_field_getter!(DummyTask, info, collection);

#[test]
fn must_task_impl_getter() {
    fn _accept_getter<T: TaskFieldGetter>(_t: MaybeUninit<T>) {}

    let dummy: MaybeUninit<DummyTask> = MaybeUninit::uninit();
    _accept_getter(dummy);
}