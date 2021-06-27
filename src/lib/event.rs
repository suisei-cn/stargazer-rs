use actix::{Handler, Message};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::to_string;

#[derive(Message, Serialize, Deserialize)]
#[rtype(result = "Result<()>")]
pub struct Event<T: Serialize + Send> {
    event_type: String,
    data: T,
    vtuber: String,
}

impl<T: Serialize + Send> Event<T> {
    pub fn to_json(&self) -> Result<String> {
        Ok(to_string(self)?)
    }
}
