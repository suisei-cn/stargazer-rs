use std::collections::HashMap;

use hmap_serde::HLabelledMap;
use serde::{Deserialize, Serialize};

use crate::db::DBRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vtuber {
    pub name: String,
    #[serde(flatten)]
    pub fields: HashMap<String, DBRef>,
}

impl Vtuber {
    pub async fn unfold<L>(&self) -> VtuberFlatten<L> {
        // TODO need type level hlist map
        // TODO HListType::map(for<T> || fields.filter_by_name(T::KEY).ok().map(|db_ref| -> T {db_ref.get()}))
        todo!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VtuberFlatten<L> {
    pub name: String,
    #[serde(
        flatten,
        bound(
            serialize = "HLabelledMap<L>: Serialize",
            deserialize = "HLabelledMap<L>: Deserialize<'de>"
        )
    )]
    pub fields: HLabelledMap<L>,
}
