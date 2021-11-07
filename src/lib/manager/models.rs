use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::db::DBRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vtuber {
    pub name: String,
    #[serde(flatten)]
    pub fields: HashMap<String, DBRef>,
}
