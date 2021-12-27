use std::collections::HashMap;
use std::marker::PhantomData;

use actix::{Actor, Addr, Message, ResponseFuture};

use crate::db::DBResult;
use crate::scheduler::models::TaskInfo;
use crate::scheduler::ops::ScheduleMode;

#[derive(Debug, Copy, Clone)]
pub struct TrySchedule<T>(pub(crate) ScheduleMode, PhantomData<T>);

impl<T> TrySchedule<T> {
    pub fn new(field0: ScheduleMode) -> Self {
        Self(field0, PhantomData)
    }
}

unsafe impl<T> Send for TrySchedule<T> {}

impl<T: Actor> Message for TrySchedule<T> {
    type Result = DBResult<Option<(TaskInfo, Addr<T>)>>;
}

/// Check whether this worker still owns the resource.
/// Returns `false` if the resource bound to this worker is replaced by another worker or deleted
#[derive(Debug, Copy, Clone, Message)]
#[rtype("DBResult<bool>")]
pub struct CheckOwnership {
    pub info: TaskInfo,
}

/// Update the timestamp.
/// Returns `false` if the resource bound to this worker is replaced by another worker or deleted
#[derive(Debug, Clone, Message)]
#[rtype("DBResult<bool>")]
pub struct UpdateEntry<T> {
    pub info: TaskInfo,
    pub body: Option<T>,
}

impl<T> UpdateEntry<T> {
    pub const fn new(info: TaskInfo, body: T) -> Self {
        Self {
            info,
            body: Some(body),
        }
    }
}

impl UpdateEntry<()> {
    pub const fn empty_payload(info: TaskInfo) -> Self {
        Self { info, body: None }
    }
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct UpdateAll {
    pub evict: bool,
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("uuid::Uuid")]
pub struct GetId;

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct TriggerGC;

#[derive(Debug, Copy, Clone)]
pub struct ActorsIter<A, F, Output>
where
    F: FnOnce(HashMap<TaskInfo, Addr<A>>) -> ResponseFuture<Output>,
    A: Actor,
{
    inner: F,
    __marker: PhantomData<(A, Output)>,
}

impl<A, F, Output> ActorsIter<A, F, Output>
where
    F: FnOnce(HashMap<TaskInfo, Addr<A>>) -> ResponseFuture<Output>,
    A: Actor,
{
    pub fn new(inner: F) -> Self {
        Self {
            inner,
            __marker: PhantomData,
        }
    }
}

impl<A, F, Output> Message for ActorsIter<A, F, Output>
where
    A: Actor,
    F: FnOnce(HashMap<TaskInfo, Addr<A>>) -> ResponseFuture<Output>,
    Output: 'static,
{
    type Result = Output;
}

impl<A, F, Output> ActorsIter<A, F, Output>
where
    F: FnOnce(HashMap<TaskInfo, Addr<A>>) -> ResponseFuture<Output>,
    A: Actor,
{
    pub fn into_inner(self) -> F {
        self.inner
    }
}
