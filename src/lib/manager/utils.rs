use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Deref;

use frunk_core::hlist::HMappable;
use frunk_core::traits::{Func, Poly};
use hmap_serde::Labelled;
use serde::{Deserialize, Deserializer, Serialize};

use crate::db::DBRef;
use crate::manager::Source;
use crate::scheduler::Task;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DBRefWrapped<T>(DBRef, PhantomData<T>);

impl<T> Deref for DBRefWrapped<T> {
    type Target = DBRef;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Task> Labelled for DBRefWrapped<T> {
    const KEY: &'static str = T::Entry::KEY;
}

pub trait ToOptionHList {
    type OptionHList;
    fn to_option_hlist(self) -> Self::OptionHList;
}

pub struct OptionLift;

impl<T: Task> Func<Source<T>> for OptionLift {
    type Output = Option<T::Entry>;

    fn call(_: Source<T>) -> Self::Output {
        None
    }
}

impl<T> ToOptionHList for T
where
    T: HMappable<Poly<OptionLift>>,
{
    type OptionHList = T::Output;

    fn to_option_hlist(self) -> Self::OptionHList {
        self.map(Poly(OptionLift))
    }
}

pub fn deserialize_maybe_hashmap<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    HashMap<K, V>: Deserialize<'de>,
{
    let maybe = Option::deserialize(deserializer)?;
    Ok(maybe.unwrap_or_default())
}
