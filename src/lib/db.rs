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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct DBRef {
    #[serde(rename = "$ref")]
    pub collection: String,
    #[serde(rename = "$id")]
    pub id: ObjectId,
    #[serde(rename = "$db", skip_serializing_if = "Option::is_none")]
    pub db: Option<String>,
}

impl DBRef {
    pub const fn get<T>(&self) -> DBRefGetOp<T> {
        DBRefGetOp(self, PhantomData)
    }
    // TODO DBRefUpdateOp ?
    pub const fn set<T>(&self, data: T) -> DBRefSetOp<T> {
        DBRefSetOp(self, data)
    }
    pub const fn del(&self) -> DBRefDelOp {
        DBRefDelOp(self)
    }
}

fn assert_not_cross_db(db_ref: &DBRef, db_target: &Database) {
    if let Some(name) = &db_ref.db {
        assert_eq!(
            name,
            db_target.name(),
            "cross db reference is not supported"
        );
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DBRefGetOp<'a, T>(&'a DBRef, PhantomData<T>);

#[async_trait]
impl<T> DBOperation for DBRefGetOp<'_, T>
where
    T: DeserializeOwned + Send + Sync + Unpin,
{
    type Result = Option<T>;

    const DESC: &'static str = "DBRefGet";

    async fn execute_impl(self, db: &Database) -> DBResult<Option<T>> {
        assert_not_cross_db(self.0, db);
        db.collection(self.0.collection.as_ref())
            .find_one(doc! {"_id": self.0.id}, None)
            .await
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DBRefSetOp<'a, T>(&'a DBRef, T);

#[async_trait]
impl<T> DBOperation for DBRefSetOp<'_, T>
where
    T: Serialize + DeserializeOwned + Send + Sync + Unpin,
{
    type Result = Option<T>;

    const DESC: &'static str = "DBRefSet";

    async fn execute_impl(self, db: &Database) -> DBResult<Option<T>> {
        assert_not_cross_db(self.0, db);
        db.collection(self.0.collection.as_ref())
            .find_one_and_replace(doc! {"_id": self.0.id}, self.1, None)
            .await
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DBRefDelOp<'a>(&'a DBRef);

#[async_trait]
impl DBOperation for DBRefDelOp<'_> {
    type Result = bool;

    const DESC: &'static str = "DBRefDel";

    async fn execute_impl(self, db: &Database) -> DBResult<bool> {
        assert_not_cross_db(self.0, db);
        db.collection::<Document>(self.0.collection.as_ref())
            .delete_one(doc! {"_id": self.0.id}, None)
            .await
            .map(|res| res.deleted_count > 0)
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
    const DESC: &'static str;
    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result>;
    async fn execute<T: Sync>(self, collection: &Collection<T>) -> DBResult<Self::Result> {
        let _timeout_guard = CancelOnDrop::new(actix::spawn(async {
            actix_rt::time::sleep(Duration::from_secs(1)).await;
            warn!("{} coll op blocked for more than 1 secs", Self::DESC);
        }));
        let _log_guard = CustomGuard::new(|| trace!("{} coll op completed", Self::DESC));

        trace!("{} coll op started", Self::DESC);
        self.execute_impl(&collection.clone_with_type()).await
    }
}

#[async_trait]
pub trait DBOperation: Sized {
    type Result;
    const DESC: &'static str;
    async fn execute_impl(self, db: &Database) -> DBResult<Self::Result>;
    async fn execute(self, db: &Database) -> DBResult<Self::Result> {
        let _timeout_guard = CancelOnDrop::new(actix::spawn(async {
            actix_rt::time::sleep(Duration::from_secs(1)).await;
            warn!("{} db op blocked for more than 1 secs", Self::DESC);
        }));
        let _log_guard = CustomGuard::new(|| trace!("{} db op completed", Self::DESC));

        trace!("{} db op started", Self::DESC);
        self.execute_impl(db).await
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
