use actix::Message;
use serde::{Serialize, Deserialize};
use serde_json::to_string;

use crate::error::Result;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Vtuber {
    key: String,
    youtube: Option<String>,
    twitter: Option<usize>,
    bilibili: Option<usize>,
}

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
#[rtype(result = "Result<()>")]
pub struct Update<T> {
    old: T,
    new: T,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "Result<()>")]
pub struct Event<T> {
    event_type: String,
    data: T,
    vtuber: Vtuber,
}

impl<T: Serialize + Send> Event<T> {
    pub fn json(&self) -> Result<String> {
        Ok(to_string(self)?)
    }
}
