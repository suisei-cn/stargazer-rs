use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TaskInfo {
    #[serde(rename = "_id")]
    doc_id: ObjectId,
    uuid: Uuid,
    parent_uuid: Uuid,
}

impl TaskInfo {
    pub const fn new(doc_id: ObjectId, uuid: Uuid, parent_uuid: Uuid) -> Self {
        Self {
            doc_id,
            uuid,
            parent_uuid,
        }
    }
    pub const fn doc_id(&self) -> ObjectId {
        self.doc_id
    }
    pub const fn uuid(&self) -> Uuid {
        self.uuid
    }
    pub const fn parent_uuid(&self) -> Uuid {
        self.parent_uuid
    }
}
