use std::mem::MaybeUninit;

use actix::{Actor, Context};
use actix_signal::SignalHandler;
use hmap_serde::Labelled;
use serde::{Deserialize, Serialize};
use tracing::{info_span, Span};

use crate::db::{Collection, Document};
use crate::scheduler::{Entry, Task, TaskFieldGetter};
use crate::utils::Scheduler;

use super::models::TaskInfo;

#[derive(Debug, Clone, SignalHandler)]
struct DummyTask {
    info: TaskInfo,
    collection: Collection<Document>,
    scheduler: Scheduler<Self>,
}

impl_task_field_getter!(DummyTask, info, scheduler);

impl Actor for DummyTask {
    type Context = Context<Self>;
}

#[derive(Debug, Serialize, Deserialize)]
struct DummyEntry;

impl Labelled for DummyEntry {
    const KEY: &'static str = "dummy";
}

impl Task for DummyTask {
    type Entry = DummyEntry;
    type Ctor = ();

    fn query() -> Document {
        unreachable!()
    }

    fn construct(
        _entry: Entry<Self::Entry>,
        _ctor: Self::Ctor,
        _scheduler: Scheduler<Self>,
        _info: TaskInfo,
        _collection: Collection<Document>,
    ) -> Self {
        unreachable!()
    }

    fn span(&self) -> Span {
        info_span!("dummy")
    }
}

#[test]
fn must_task_impl_getter() {
    fn _accept_getter<T: TaskFieldGetter>(_t: &MaybeUninit<T>) {}

    let dummy: MaybeUninit<DummyTask> = MaybeUninit::uninit();
    _accept_getter(&dummy);
}
