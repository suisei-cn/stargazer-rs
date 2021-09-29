use std::collections::HashMap;
use std::marker::PhantomData;

use actix::{Actor, Addr, Message, ResponseFuture};
use uuid::Uuid;

use crate::db::DBResult;
use crate::scheduler::models::TaskInfo;

#[derive(Debug, Copy, Clone)]
pub struct TrySchedule<T>(PhantomData<T>);

impl<T: Actor> Message for TrySchedule<T> {
    type Result = DBResult<Option<(Uuid, Addr<T>)>>;
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
        UpdateEntry {
            info,
            body: body.into(),
        }
    }
}

impl UpdateEntry<()> {
    pub fn empty_payload(info: TaskInfo) -> Self {
        UpdateEntry { info, body: None }
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

impl<T> Default for TrySchedule<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> TrySchedule<T> {
    pub fn new() -> Self {
        Default::default()
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
