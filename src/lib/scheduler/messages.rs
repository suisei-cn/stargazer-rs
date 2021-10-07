use std::collections::HashMap;
use std::marker::PhantomData;

use actix::{Actor, Addr, Message, ResponseFuture};
use uuid::Uuid;

use crate::db::DBResult;
use crate::scheduler::models::TaskInfo;
use crate::scheduler::ops::ScheduleMode;

#[derive(Debug, Copy, Clone)]
pub struct TrySchedule<T>(pub(crate) ScheduleMode, PhantomData<T>);

impl<T: Actor> Message for TrySchedule<T> {
    type Result = DBResult<Vec<(Uuid, Addr<T>)>>;
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
    pub fn new(info: TaskInfo, body: impl Into<Option<T>>) -> Self {
        Self {
            info,
            body: body.into(),
        }
    }
}

impl UpdateEntry<()> {
    pub const fn empty_payload(info: TaskInfo) -> Self {
        Self { info, body: None }
    }
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
    F: FnOnce(HashMap<Uuid, Addr<A>>) -> ResponseFuture<Output>,
    A: Actor,
{
    inner: F,
    __marker_1: PhantomData<A>,
    __marker_2: PhantomData<Output>,
}

impl<A, F, Output> Message for ActorsIter<A, F, Output>
where
    A: Actor,
    F: FnOnce(HashMap<Uuid, Addr<A>>) -> ResponseFuture<Output>,
    Output: 'static,
{
    type Result = Output;
}

impl<T> TrySchedule<T> {
    pub const fn new(mode: ScheduleMode) -> Self {
        Self(mode, PhantomData)
    }
}

impl<A, F, Output> ActorsIter<A, F, Output>
where
    F: FnOnce(HashMap<Uuid, Addr<A>>) -> ResponseFuture<Output>,
    A: Actor,
{
    pub fn new(f: F) -> Self {
        Self {
            inner: f,
            __marker_1: PhantomData,
            __marker_2: PhantomData,
        }
    }
    pub fn into_inner(self) -> F {
        self.inner
    }
}
