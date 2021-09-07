use async_trait::async_trait;
use mongodb::bson::Document;
use mongodb::error::Result as DBResult;
use mongodb::Collection;

#[async_trait]
pub trait DBOperation {
    type Result;
    async fn execute(&self, collection: &Collection<Document>) -> DBResult<Self::Result>;
}
