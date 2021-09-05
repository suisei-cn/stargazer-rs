#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::future_not_send)] // we are using actix-rt, a single-threaded runtime

pub use context::{ArbiterContext, InstanceContext};
pub use error::Error;
pub use event_source::*;
pub use model::*;
pub use server::*;

pub use crate::config::*;

#[macro_use]
mod utils;

mod api;
mod config;
pub mod context;
mod error;
mod event_source;
mod model;
pub mod request;
mod server;
pub mod worker;

mod common;
#[cfg(test)]
mod tests;
