use std::collections::HashMap;
use std::marker::PhantomData;

use actix::{Actor, Addr, Message, ResponseFuture};
use derive_new::new;
use getset::CopyGetters;

use crate::db::DBResult;
use crate::scheduler::models::TaskInfo;
use crate::scheduler::ops::ScheduleMode;

#[derive(Debug, Copy, Clone, new)]
pub struct TrySchedule<T>(pub(crate) ScheduleMode, #[new(default)] PhantomData<T>);

unsafe impl<T> Send for TrySchedule<T> {}

impl<T: Actor> Message for TrySchedule<T> {
    type Result = DBResult<Option<(TaskInfo, Addr<T>)>>;
}

/// Check whether this worker still owns the resource.
/// Returns `false` if the resource bound to this worker is replaced by another worker or deleted
#[derive(Debug, Copy, Clone, Message, new, CopyGetters)]
#[rtype("DBResult<bool>")]
#[getset(get_copy = "pub")]
pub struct CheckOwnership {
    info: TaskInfo,
}

/// Update the timestamp.
/// Returns `false` if the resource bound to this worker is replaced by another worker or deleted
#[derive(Debug, Clone, Message)]
#[rtype("DBResult<bool>")]
pub struct UpdateEntry<T> {
    pub(crate) info: TaskInfo,
    pub(crate) body: Option<T>,
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

#[derive(Debug, Copy, Clone, Message, new, CopyGetters)]
#[rtype("()")]
#[getset(get_copy = "pub")]
pub struct UpdateAll {
    evict: bool,
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("uuid::Uuid")]
pub struct GetId;

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct TriggerGC;

#[derive(Debug, Copy, Clone, new)]
pub struct ActorsIter<A, F, Output>
where
    F: FnOnce(HashMap<TaskInfo, Addr<A>>) -> ResponseFuture<Output>,
    A: Actor,
{
    inner: F,
    #[new(default)]
    __marker_1: PhantomData<A>,
    #[new(default)]
    __marker_2: PhantomData<Output>,
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
