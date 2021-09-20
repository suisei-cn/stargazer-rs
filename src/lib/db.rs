use async_trait::async_trait;
pub use mongodb::bson::Document;
pub use mongodb::error::Result as DBResult;
use mongodb::{Client, Database};

pub type Collection = mongodb::Collection<mongodb::bson::Document>;

#[async_trait]
pub trait DBOperation {
    type Result;
    async fn execute(&self, collection: &Collection) -> DBResult<Self::Result>;
}

pub async fn connect_db(db_uri: &str, db_name: &str) -> DBResult<Database> {
    Client::with_uri_str(db_uri)
        .await
        .map(|client| client.database(db_name))
}
