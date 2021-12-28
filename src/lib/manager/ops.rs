use std::collections::HashMap;

use async_trait::async_trait;
use erased_serde::private::serde::Serialize;
use mongodb::bson;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

use crate::db::{CollOperation, Collection, DBRef, DBResult, Document};
use crate::utils::DBErrorExt;

use super::models::Vtuber;

#[derive(Debug)]
pub struct GetVtuberOp {
    pub name: String,
}

#[async_trait]
impl CollOperation for GetVtuberOp {
    type Result = Option<Vtuber>;
    type Item = Vtuber;

    const DESC: &'static str = "GetVtuber";

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        collection.find_one(doc! {"name": self.name}, None).await
    }
}

#[derive(Debug)]
pub struct CreateVtuberOp {
    pub name: String,
}

#[async_trait]
impl CollOperation for CreateVtuberOp {
    type Result = bool;
    type Item = Vtuber;

    const DESC: &'static str = "CreateVtuber";

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        collection
            .insert_one(
                Vtuber {
                    doc_id: ObjectId::new(),
                    name: self.name,
                    fields: HashMap::new(),
                },
                None,
            )
            .await
            .map(|_| true)
            .or_else(|e| (e.write() == Some(11000)).then(|| false).ok_or(e))
    }
}

#[derive(Debug)]
pub struct DeleteVtuberOp {
    pub name: String,
}

#[async_trait]
impl CollOperation for DeleteVtuberOp {
    type Result = bool;
    type Item = Vtuber;

    const DESC: &'static str = "CreateVtuber";

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        collection
            .delete_one(doc! {"name": self.name}, None)
            .await
            .map(|res| res.deleted_count > 0)
    }
}

#[derive(Debug)]
pub struct CreateFieldOp<T>(pub T);

#[async_trait]
impl<T> CollOperation for CreateFieldOp<T>
where
    T: Serialize + Send + Sync,
{
    type Result = DBRef;
    type Item = T;
    const DESC: &'static str = "SetField";

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        collection.insert_one(self.0, None).await.map(|res| DBRef {
            collection: collection.name().to_string(),
            id: bson::from_bson(res.inserted_id).unwrap(), // SAFETY ensured by mongodb
            db: None,
        })
    }
}

#[derive(Debug)]
pub struct LinkRefOp {
    name: String,
    field: &'static str,
    db_ref: Option<DBRef>,
}

impl LinkRefOp {
    pub const fn new(name: String, field: &'static str, db_ref: Option<DBRef>) -> Self {
        Self {
            name,
            field,
            db_ref,
        }
    }
}

#[async_trait]
impl CollOperation for LinkRefOp {
    type Result = ();
    type Item = Document;
    const DESC: &'static str = "LinkRef";

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        let field = self.field;
        let update_doc = self.db_ref.map_or_else(
            || {
                doc! {
                    "$unset": {
                        "fields": {
                            field: ""
                        }
                    }
                }
            },
            |db_ref| {
                doc! {
                    "$set": {
                        "fields": {
                            field: bson::to_bson(&db_ref).unwrap()
                        }
                    }
                }
            },
        );
        collection
            .find_one_and_update(
                doc! {
                    "name": self.name
                },
                update_doc,
                None,
            )
            .await
            .map(|_| ())
    }
}
