use std::marker::PhantomData;
use std::ops::Deref;
use std::time::Duration;

use async_trait::async_trait;
use derive_new::new;
use mongodb::bson::oid::ObjectId;
pub use mongodb::bson::{doc, Document};
pub use mongodb::error::Result as DBResult;
pub use mongodb::Collection;
use mongodb::{Client, Database};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{trace, warn};

use crate::utils::{CancelOnDrop, CustomGuard};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DBRef {
    #[serde(rename = "$ref")]
    pub collection: String,
    #[serde(rename = "$id")]
    pub id: ObjectId,
    #[serde(rename = "$db", skip_serializing_if = "Option::is_none")]
    pub db: Option<String>,
}

impl DBRef {
    pub fn lookup<T>(&self) -> DBRefLookupOp<T> {
        DBRefLookupOp(self, PhantomData)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DBRefLookupOp<'a, T>(&'a DBRef, PhantomData<T>);

#[async_trait]
impl<T> DBOperation for DBRefLookupOp<'_, T>
where
    T: DeserializeOwned + Send + Sync + Unpin,
{
    type Result = Option<T>;

    fn desc() -> &'static str {
        "DBRefLookup"
    }

    async fn execute_impl(self, db: &Database) -> DBResult<Option<T>> {
        if self
            .0
            .db
            .as_ref()
            .map(|name| db.name() != name)
            .unwrap_or(false)
        {
            panic!("cross db reference not supported");
        }
        db.collection(self.0.collection.as_ref())
            .find_one(doc! {"_id": self.0.id}, None)
            .await
    }
}

#[derive(new)]
pub struct Coll<T> {
    coll: Collection<Document>,
    #[new(default)]
    _marker: PhantomData<T>,
}

impl<T> Deref for Coll<T> {
    type Target = Collection<Document>;

    fn deref(&self) -> &Self::Target {
        &self.coll
    }
}

#[async_trait]
pub trait CollOperation: Sized {
    type Result;
    type Item: Send + Sync;
    fn desc() -> &'static str;
    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result>;
    async fn execute<T: Sync>(self, collection: &Collection<T>) -> DBResult<Self::Result> {
        let _timeout_guard = CancelOnDrop::new(actix::spawn(async {
            actix_rt::time::sleep(Duration::from_secs(1)).await;
            warn!("{} coll op blocked for more than 1 secs", Self::desc());
        }));
        let _log_guard = CustomGuard::new(|| trace!("{} coll op completed", Self::desc()));

        trace!("{} coll op started", Self::desc());
        self.execute_impl(&collection.clone_with_type()).await
    }
}

#[async_trait]
pub trait DBOperation: Sized {
    type Result;
    fn desc() -> &'static str;
    async fn execute_impl(self, db: &Database) -> DBResult<Self::Result>;
    async fn execute(self, db: &Database) -> DBResult<Self::Result> {
        let _timeout_guard = CancelOnDrop::new(actix::spawn(async {
            actix_rt::time::sleep(Duration::from_secs(1)).await;
            warn!("{} db op blocked for more than 1 secs", Self::desc());
        }));
        let _log_guard = CustomGuard::new(|| trace!("{} db op completed", Self::desc()));

        trace!("{} db op started", Self::desc());
        self.execute(db).await
    }
}

/// Connect to mongodb database.
///
/// # Errors
/// Pass errors raised by mongodb driver.
pub async fn connect_db(db_uri: &str, db_name: &str) -> DBResult<Database> {
    Client::with_uri_str(db_uri)
        .await
        .map(|client| client.database(db_name))
}
