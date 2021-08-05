use std::collections::HashMap;
use std::marker::PhantomData;

use actix::{Actor, Addr, Message, ResponseFuture};
use uuid::Uuid;

#[derive(Debug, Copy, Clone)]
pub struct TrySteal<T>(PhantomData<T>);

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

impl<T> Default for TrySteal<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> TrySteal<T> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<T: Actor> Message for TrySteal<T> {
    type Result = Option<(Uuid, Addr<T>)>;
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

impl<A, F, Output> Message for ActorsIter<A, F, Output>
where
    A: Actor,
    F: FnOnce(HashMap<Uuid, Addr<A>>) -> ResponseFuture<Output>,
    Output: 'static,
{
    type Result = Output;
}
