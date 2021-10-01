use std::marker::PhantomData;
use std::ops::Deref;
use std::time::Duration;

use async_trait::async_trait;
pub use mongodb::bson::Document;
pub use mongodb::error::Result as DBResult;
pub use mongodb::Collection;
use mongodb::{Client, Database};
use tracing::{trace, warn};

use crate::utils::{CancelOnDrop, CustomGuard};

pub struct Coll<T> {
    coll: Collection<Document>,
    _marker: PhantomData<T>,
}

impl<T> Coll<T> {
    pub const fn new(coll: Collection<Document>) -> Self {
        Self {
            coll,
            _marker: PhantomData,
        }
    }
}

impl<T> Deref for Coll<T> {
    type Target = Collection<Document>;

    fn deref(&self) -> &Self::Target {
        &self.coll
    }
}

#[async_trait]
pub trait DBOperation {
    type Result;
    type Item: Send + Sync;
    fn desc() -> &'static str;
    async fn execute_impl(&self, collection: &Collection<Self::Item>) -> DBResult<Self::Result>;
    async fn execute<T: Sync>(&self, collection: &Collection<T>) -> DBResult<Self::Result> {
        let _timeout_guard = CancelOnDrop::new(actix::spawn(async {
            actix_web::rt::time::sleep(Duration::from_secs(1)).await;
            warn!("{} op blocked for more than 1 secs", Self::desc());
        }));
        let _log_guard = CustomGuard::new(|| trace!("{} op completed", Self::desc()));

        trace!("{} op started", Self::desc());
        self.execute_impl(&collection.clone_with_type()).await
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

pub fn transmute_collection<T, U>(coll: Collection<T>) -> Collection<U> {
    unsafe { std::mem::transmute(coll) }
}

#[allow(clippy::missing_const_for_fn)]
pub fn transmute_collection_ref<T, U>(coll: &Collection<T>) -> &Collection<U> {
    unsafe { &*(coll as *const mongodb::Collection<T>).cast() }
}
