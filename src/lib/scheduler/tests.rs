use std::mem::MaybeUninit;

use actix::{Actor, Addr, Context};

use crate::db::{Collection, Document};
use crate::scheduler::actor::ScheduleActor;
use crate::scheduler::{Task, TaskFieldGetter};

use super::models::TaskInfo;

#[derive(Debug, Clone)]
struct DummyTask {
    info: TaskInfo,
    collection: Collection,
    scheduler: Addr<ScheduleActor<DummyTask>>,
}

impl_task_field_getter!(DummyTask, info, collection, scheduler);

impl Actor for DummyTask {
    type Context = Context<Self>;
}

impl Task for DummyTask {
    type Entry = ();
    type Ctor = ();

    fn query() -> Document {
        unreachable!()
    }

    fn construct(
        _entry: Self::Entry,
        _ctor: Self::Ctor,
        _scheduler: Addr<ScheduleActor<Self>>,
        _info: TaskInfo,
        _collection: Collection,
    ) -> Self {
        unreachable!()
    }
}

#[test]
fn must_task_impl_getter() {
    fn _accept_getter<T: TaskFieldGetter>(_t: MaybeUninit<T>) {}

    let dummy: MaybeUninit<DummyTask> = MaybeUninit::uninit();
    _accept_getter(dummy);
}
