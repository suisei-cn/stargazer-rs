use async_trait::async_trait;
pub use mongodb::bson::Document;
pub use mongodb::error::Result as DBResult;

pub type Collection = mongodb::Collection<mongodb::bson::Document>;

#[async_trait]
pub trait DBOperation {
    type Result;
    async fn execute(&self, collection: &Collection) -> DBResult<Self::Result>;
}
