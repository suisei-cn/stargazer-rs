use actix::{Handler, Message};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::to_string;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Vtuber {
    key: String,
    youtube: Option<String>,
    twitter: Option<usize>,
    bilibili: Option<usize>,
}

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
#[rtype(result = "Result<()>")]
pub struct Update<T> {
    old: T,
    new: T,
}

#[derive(Message, Serialize, Deserialize, Copy, Debug)]
#[rtype(result = "Result<()>")]
pub struct Event<T: Serialize + Deserialize + Send> {
    event_type: String,
    data: T,
    vtuber: Vtuber,
}

impl<T: Serialize + Send> Event<T> {
    pub fn json(&self) -> Result<String> {
        Ok(to_string(self)?)
    }
}
