use async_trait::async_trait;
use derive_new::new;
use erased_serde::private::serde::Serialize;
use mongodb::bson;
use mongodb::bson::doc;

use crate::db::{CollOperation, Collection, DBRef, DBResult, Document};

use super::models::Vtuber;

#[derive(Debug, new)]
pub struct GetVtuberOp {
    name: String,
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

#[derive(Debug, new)]
pub struct CreateFieldOp<T>(T);

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

#[derive(Debug, new)]
pub struct LinkRefOp {
    name: String,
    field: &'static str,
    db_ref: DBRef,
}

#[async_trait]
impl CollOperation for LinkRefOp {
    type Result = ();
    type Item = Document;
    const DESC: &'static str = "LinkRef";

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        let mut update_doc = Document::new();
        update_doc.insert(self.field, bson::to_bson(&self.db_ref).unwrap());
        collection
            .find_one_and_update(
                doc! {
                    "name": self.name
                },
                doc! {
                    "$set": update_doc
                },
                None,
            )
            .await
            .map(|_| ())
    }
}
