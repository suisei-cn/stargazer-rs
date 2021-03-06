use actix::Actor;
use actix_signal::SignalHandler;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::serde_helpers::uuid_as_binary;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::actor::ScheduleContext;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TaskInfo {
    #[serde(rename = "_id")]
    pub doc_id: ObjectId,
    #[serde(with = "uuid_as_binary")]
    pub uuid: Uuid,
    #[serde(with = "uuid_as_binary")]
    pub parent_uuid: Uuid,
}

#[derive(Debug, Copy, Clone)]
pub struct SchedulerMeta {
    pub id: Uuid,
    pub actor_count: usize,
}

impl<T: Actor + SignalHandler> From<&ScheduleContext<T>> for SchedulerMeta {
    fn from(ctx: &ScheduleContext<T>) -> Self {
        Self {
            id: ctx.id,
            actor_count: ctx.actors.len(),
        }
    }
}
