use actix::Actor;
use actix_signal::SignalHandler;
use derive_new::new;
use getset::CopyGetters;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::actor::ScheduleContext;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, new, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct TaskInfo {
    #[serde(rename = "_id")]
    doc_id: ObjectId,
    uuid: Uuid,
    parent_uuid: Uuid,
}

#[derive(Debug, Copy, Clone, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct SchedulerMeta {
    id: Uuid,
    actor_count: usize,
}

impl<T: Actor + SignalHandler> From<&ScheduleContext<T>> for SchedulerMeta {
    fn from(ctx: &ScheduleContext<T>) -> Self {
        Self {
            id: ctx.id(),
            actor_count: ctx.actors().len(),
        }
    }
}
