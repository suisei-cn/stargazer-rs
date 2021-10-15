use std::marker::PhantomData;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use mongodb::bson::{self, bson, doc, Document};
use mongodb::error::Result as DBResult;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use rand::seq::SliceRandom;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::{Collection, DBOperation};
use crate::utils::timestamp;

use super::models::{SchedulerMeta, TaskInfo};

pub struct CheckOwnershipOp {
    info: TaskInfo,
}

impl CheckOwnershipOp {
    pub const fn new(info: TaskInfo) -> Self {
        Self { info }
    }
}

#[async_trait]
impl DBOperation for CheckOwnershipOp {
    type Result = bool;
    type Item = TaskInfo;

    fn desc() -> &'static str {
        "CheckOwnership"
    }

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        collection
            .find_one(bson::to_document(&self.info).unwrap(), None)
            .await
            .map(|maybe| maybe.is_some())
    }
}

#[derive(Debug, Clone)]
pub struct UpdateEntryOp<T> {
    info: TaskInfo,
    body: Option<T>,
}

impl<T> UpdateEntryOp<T> {
    pub const fn new(info: TaskInfo, body: Option<T>) -> Self {
        Self { info, body }
    }
}

#[async_trait]
impl<T: Serialize + Send> DBOperation for UpdateEntryOp<T> {
    type Result = bool;
    type Item = Document;

    fn desc() -> &'static str {
        "UpdateEntry"
    }

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        let mut body = if let Some(body) = &self.body {
            bson::to_document(body)?
        } else {
            Document::new()
        };
        body.insert("timestamp", timestamp(SystemTime::now()));

        Ok(collection
            .update_one(
                doc! {"_id": self.info.doc_id(), "uuid": self.info.uuid().to_string()},
                doc! {"$set": body},
                None,
            )
            .await?
            .modified_count
            > 0)
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ScheduleMode {
    // Schedule outdated task. If failed, randomly schedule a task from another scheduler.
    Auto,
    // Schedule outdated task only.
    OutdatedOnly,
    // Steal a task preemptively.
    StealOnly,
}

// internal enum for schedule op
enum ScheduleResult<T> {
    // We've acquired the task.
    Some(T),
    // No task acquired.
    None,
    // Conflict with another scheduler.
    Conflict,
}

#[derive(Debug, Copy, Clone, Deserialize, Eq, PartialEq)]
pub struct WorkerInfo {
    #[serde(rename = "_id")]
    id: Uuid,
    count: u64,
}

impl Default for ScheduleMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone)]
pub struct GetAllTasksCount {
    base_query: Document,
}

impl GetAllTasksCount {
    pub const fn new(base_query: Document) -> Self {
        Self { base_query }
    }
}

#[async_trait]
impl DBOperation for GetAllTasksCount {
    type Result = u64;
    type Item = Document;

    fn desc() -> &'static str {
        "GetAllTasksCount"
    }

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        collection
            .count_documents(self.base_query.clone(), None)
            .await
    }
}

#[derive(Debug, Clone)]
pub struct GetWorkerInfoOp {
    base_query: Document,
    since_ts: i64,
    parent_id: Uuid,
}

#[async_trait]
impl DBOperation for GetWorkerInfoOp {
    type Result = Vec<WorkerInfo>;
    type Item = WorkerInfo;

    fn desc() -> &'static str {
        "GetWorkerInfo"
    }

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        let filter_query = doc! {
            "$match": {
                "$and": [
                    {"parent_uuid": {"$ne": self.parent_id.to_string()}},   // exclude current worker
                    {"timestamp": {"$gte": self.since_ts}},   // updated recently
                    self.base_query.clone()
                ]
            }
        };
        let count_parent_uuid = doc! {
            "$group": {
                "_id": "$parent_uuid", "count": {"$sum": 1}
            }
        };
        collection
            .aggregate([filter_query, count_parent_uuid], None)
            .await?
            .map(|doc| doc.map(|doc| -> WorkerInfo { bson::from_document(doc).unwrap() }))
            .try_collect()
            .await
    }
}

impl GetWorkerInfoOp {
    pub const fn new(base_query: Document, retry_ts: i64, parent_id: Uuid) -> Self {
        Self {
            base_query,
            since_ts: retry_ts,
            parent_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GetTasksOnWorkerOp {
    base_query: Document,
    since_ts: i64,
    worker: Uuid,
}

impl GetTasksOnWorkerOp {
    pub const fn new(base_query: Document, since_ts: i64, worker: Uuid) -> Self {
        Self {
            base_query,
            since_ts,
            worker,
        }
    }
}

#[async_trait]
impl DBOperation for GetTasksOnWorkerOp {
    type Result = Vec<TaskInfo>;
    type Item = TaskInfo;

    fn desc() -> &'static str {
        "GetTasksOnWorker"
    }

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        let filter_query = doc! {
            "$and": [
                {"parent_uuid": self.worker.to_string()},   // select worker
                {"timestamp": {"$gte": self.since_ts}},   // updated recently
                self.base_query.clone()
            ]
        };
        collection
            .find(filter_query, None)
            .await?
            .try_collect()
            .await
    }
}

#[derive(Debug, Clone)]
pub struct ScheduleOp<T> {
    mode: ScheduleMode,
    query: Document,
    update: Document,
    since_ts: i64,
    parent_meta: SchedulerMeta,
    __marker: PhantomData<T>,
}

impl<T> ScheduleOp<T> {
    pub fn new(
        mode: ScheduleMode,
        query: Document,
        parent_meta: SchedulerMeta,
        ago: Duration,
    ) -> Self {
        let since_ts = timestamp(SystemTime::now() - ago);
        let now_ts = timestamp(SystemTime::now());
        let uuid = Uuid::new_v4();
        let update = doc! {
            "$set": {
                "timestamp": now_ts,
                "uuid": uuid.to_string(),
                "parent_uuid": parent_meta.id().to_string()
            }
        };
        Self {
            mode,
            query,
            update,
            since_ts,
            parent_meta,
            __marker: PhantomData::default(),
        }
    }

    // Try to acquire an outdated entry.
    async fn do_acquire(&self, collection: &Collection<Document>) -> DBResult<Option<Document>> {
        let mut query = self.query.clone();
        query.insert(
            "$or",
            bson!([
                {
                    "timestamp": {
                        "$lt": self.since_ts
                    }
                },
                {
                    "timestamp": {
                        "$exists": false
                    }
                }
            ]),
        );
        collection
            .find_one_and_update(
                query,
                self.update.clone(),
                Some(
                    FindOneAndUpdateOptions::builder()
                        .return_document(ReturnDocument::After)
                        .build(),
                ),
            )
            .await
    }

    async fn do_schedule_once(
        &self,
        collection: &Collection<Document>,
    ) -> DBResult<ScheduleResult<Document>> {
        let entries_count = GetAllTasksCount::new(self.query.clone())
            .execute(collection)
            .await?;
        let workers =
            GetWorkerInfoOp::new(self.query.clone(), self.since_ts, self.parent_meta.id())
                .execute(collection)
                .await?;

        // allowed entries per worker is [expected, expected+1].
        let self_count = self.parent_meta.actor_count() as u64;
        let workers_count = workers.len() as u64;
        let expected = entries_count / (workers_count + 1);
        let threshold = if self_count < expected {
            // If we are underloaded (self < expected), we may steal from those workers.
            expected
        } else {
            // Those are the overloaded ones, steal them if (self == expected).
            expected + 1
        };

        let victim_worker = if self_count <= expected {
            workers
                .into_iter()
                .filter(|worker| worker.count > threshold)
                .collect::<Vec<_>>()
                .choose(&mut rand::thread_rng())
                .copied()
        } else {
            None
        };

        Ok(if let Some(victim_worker) = victim_worker {
            let tasks =
                GetTasksOnWorkerOp::new(self.query.clone(), self.since_ts, victim_worker.id)
                    .execute(collection)
                    .await?;

            if tasks.len() as u64 > threshold {
                info!("steal one entry");
                let victim_task = tasks.choose(&mut rand::thread_rng()).unwrap();
                collection
                    .find_one_and_update(
                        bson::to_document(victim_task).unwrap(),
                        self.update.clone(),
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await?
                    .map_or(ScheduleResult::Conflict, ScheduleResult::Some)
            } else {
                // target worker has insufficient tasks, indicating a steal conflict
                ScheduleResult::Conflict
            }
        } else {
            // no need to steal anything
            ScheduleResult::None
        })
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send + Sync> DBOperation for ScheduleOp<T> {
    type Result = Option<(TaskInfo, T)>;
    type Item = Document;

    fn desc() -> &'static str {
        "Schedule"
    }

    async fn execute_impl(self, collection: &Collection<Self::Item>) -> DBResult<Self::Result> {
        Ok(match self.mode {
            ScheduleMode::Auto => {
                if let Some(res) = self.do_acquire(collection).await? {
                    Some(res)
                } else {
                    loop {
                        match self.do_schedule_once(collection).await? {
                            ScheduleResult::Conflict => {
                                warn!("steal conflict, retry");
                                continue;
                            }
                            ScheduleResult::Some(res) => break Some(res),
                            ScheduleResult::None => break None,
                        }
                    }
                }
            }
            ScheduleMode::OutdatedOnly => self.do_acquire(collection).await?,
            ScheduleMode::StealOnly => loop {
                match self.do_schedule_once(collection).await? {
                    ScheduleResult::Conflict => {
                        warn!("steal conflict, retry");
                        continue;
                    }
                    ScheduleResult::Some(res) => break Some(res),
                    ScheduleResult::None => break None,
                }
            },
        }
        .map(|res| {
            (
                bson::from_document(res.clone()).unwrap(),
                bson::from_document(res).unwrap(),
            )
        }))
    }
}
