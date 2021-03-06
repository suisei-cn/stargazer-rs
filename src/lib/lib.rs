#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::future_not_send)] // we are using actix-rt, a single-threaded runtime

pub use context::{ArbiterContext, InstanceContext};
pub use error::Error;
pub use server::*;

pub use crate::config::*;

#[macro_use]
pub mod utils;

mod config;
pub mod context;
mod error;
pub mod request;
mod server;

pub mod collector;
mod common;
pub mod db;
pub mod manager;
pub mod scheduler;
pub mod source;
#[cfg(test)]
mod tests;
