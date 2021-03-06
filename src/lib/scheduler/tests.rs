use std::convert::Infallible;
use std::fmt::{Display, Formatter};
use std::mem::MaybeUninit;
use std::str::FromStr;

use actix::{Actor, Context};
use actix_signal::SignalHandler;
use hmap_serde::Labelled;
use serde::{Deserialize, Serialize};
use tracing::{info_span, Span};

use crate::db::Document;
use crate::scheduler::{Entry, Task, TaskFieldGetter};
use crate::utils::Scheduler;

use super::models::TaskInfo;

#[derive(Debug, Clone, SignalHandler)]
struct DummyTask {
    info: TaskInfo,
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

impl FromStr for DummyEntry {
    type Err = Infallible;

    fn from_str(_: &str) -> Result<Self, Self::Err> {
        Ok(Self)
    }
}

impl Display for DummyEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
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
