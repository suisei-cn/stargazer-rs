use std::collections::HashMap;

use hmap_serde::HLabelledMap;
use mongodb::bson::oid::ObjectId;
use mongodb::Database;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::db::{DBOperation, DBRef, DBResult, Document};

use super::utils::deserialize_maybe_hashmap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vtuber {
    #[serde(rename = "_id")]
    pub doc_id: ObjectId,
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_maybe_hashmap")]
    pub fields: HashMap<String, DBRef>,
}

impl Vtuber {
    /// Follow untyped vtuber field references and return typed concrete data.
    ///
    /// # Errors
    /// Forward underlying database errors.
    /// Also raise errors if can't fit fields into given heterogeneous list.
    pub async fn flatten<L>(&self, db: &Database) -> DBResult<VtuberFlatten<L>>
    where
        HLabelledMap<L>: DeserializeOwned,
    {
        let mut erased_fields = Document::new();
        for (key, db_ref) in &self.fields {
            erased_fields.insert(key, db_ref.get::<Document>().execute(db).await?);
        }

        Ok(VtuberFlatten {
            name: self.name.clone(),
            fields: mongodb::bson::from_document(erased_fields)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VtuberFlatten<L> {
    pub name: String,
    #[serde(bound(
        serialize = "HLabelledMap<L>: Serialize",
        deserialize = "HLabelledMap<L>: Deserialize<'de>"
    ))]
    pub fields: HLabelledMap<L>,
}
