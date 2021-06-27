#![feature(associated_type_bounds)]

mod app;
mod client;
mod dispatcher;
mod event;
mod util;
mod websocket;

pub use app::*;
pub use client::*;
pub use dispatcher::*;
pub use event::*;
pub use util::*;

// TODO: add more prelude staff
pub mod prelude {
    use super::app::App;
}
