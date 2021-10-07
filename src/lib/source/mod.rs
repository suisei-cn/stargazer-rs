use actix::Message;
use serde::Serialize;

pub mod bililive;
pub mod debug;
pub mod twitter;

#[derive(Debug, Clone, Message)]
#[rtype("()")]
pub struct ToCollector<T: Serialize> {
    topic: String,
    body: T,
}

impl<T: Serialize> ToCollector<T> {
    pub fn new(topic: &str, body: T) -> Self {
        Self {
            topic: topic.to_string(),
            body,
        }
    }
}
