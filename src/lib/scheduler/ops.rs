use std::marker::PhantomData;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use log::info;
use mongodb::bson::{self, bson, doc, Document};
use mongodb::error::Result as DBResult;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use rand::seq::SliceRandom;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use uuid::Uuid;

use crate::db::{Collection, DBOperation};

use super::models::{SchedulerMeta, TaskInfo};

#[derive(Debug, Clone)]
pub struct UpdateTSOp {
    task_info: TaskInfo,
}

impl UpdateTSOp {
    pub fn new(task_info: TaskInfo) -> Self {
        UpdateTSOp { task_info }
    }
}

#[async_trait]
impl DBOperation for UpdateTSOp {
    type Result = bool;

    async fn execute(&self, collection: &Collection) -> DBResult<Self::Result> {
        Ok(collection
            .update_one(doc! {"_id": self.task_info.doc_id(), "uuid": self.task_info.uuid().to_string()},
                        doc! {"$set": {"timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64}},
                        None)
            .await?
            .modified_count > 0)
    }
}

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
enum ScheduleResult {
    // We've acquired the task.
    Some(Document),
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
        ScheduleMode::Auto
    }
}

#[derive(Debug, Clone)]
pub struct GetAllTasksCount {
    base_query: Document,
}

impl GetAllTasksCount {
    pub fn new(base_query: Document) -> Self {
        GetAllTasksCount { base_query }
    }
}

#[async_trait]
impl DBOperation for GetAllTasksCount {
    type Result = u64;

    async fn execute(&self, collection: &Collection) -> DBResult<Self::Result> {
        collection
            .count_documents(self.base_query.clone(), None)
            .await
    }
}

#[derive(Debug, Clone)]
pub struct GetWorkerInfoOp {
    base_query: Document,
    retry_ts: i64,
    parent_meta: SchedulerMeta,
}

#[async_trait]
impl DBOperation for GetWorkerInfoOp {
    type Result = Vec<WorkerInfo>;

    async fn execute(&self, collection: &Collection) -> DBResult<Self::Result> {
        let filter_query = doc! {
            "$match": {
                "$and": [
                    {"parent_uuid": {"$ne": self.parent_meta.id().to_string()}},   // exclude current worker
                    {"timestamp": {"$gte": self.retry_ts}},   // updated recently
                    self.base_query.clone()
                ]
            }
        };
        let count_parent_uuid = doc! {
            "$group": {
                "_id": "$parent_uuid", "count": {"$sum": 1}
            }
        };
        Ok(collection
            .aggregate([filter_query, count_parent_uuid], None)
            .await?
            .map(|doc| doc.map(|doc| -> WorkerInfo { bson::from_document(doc).unwrap() }))
            .try_collect()
            .await?)
    }
}

impl GetWorkerInfoOp {
    pub fn new(base_query: Document, retry_ts: i64, parent_meta: SchedulerMeta) -> Self {
        GetWorkerInfoOp {
            base_query,
            retry_ts,
            parent_meta,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScheduleOp<T> {
    mode: ScheduleMode,
    query: Document,
    update: Document,
    retry_ts: i64,
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
        let retry_ts = (SystemTime::now() - ago)
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let uuid = Uuid::new_v4();
        let update = doc! {
            "$set": {
                "timestamp": ts,
                "uuid": uuid.to_string(),
                "parent_uuid": parent_meta.id().to_string()
            }
        };
        Self {
            mode,
            query,
            update,
            retry_ts,
            parent_meta,
            __marker: PhantomData::default(),
        }
    }

    // Try to acquire an outdated entry.
    async fn do_acquire(&self, collection: &Collection) -> DBResult<Option<Document>> {
        let mut query = self.query.clone();
        query.insert(
            "$or",
            bson!([
                {
                    "timestamp": {
                        "$lt": self.retry_ts
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

    async fn do_schedule_once(&self, collection: &Collection) -> DBResult<ScheduleResult> {
        let entries_count = GetAllTasksCount::new(self.query.clone())
            .execute(collection)
            .await?;
        let workers = GetWorkerInfoOp::new(self.query.clone(), self.retry_ts, self.parent_meta)
            .execute(collection)
            .await?;

        // allowed entries per worker is [expected, expected+1].
        let self_count = self.parent_meta.actor_count() as u64;
        let workers_count = workers.len() as u64;
        let expected = entries_count / (workers_count + 1);

        let victim_worker = if self_count <= expected {
            workers
                .into_iter()
                .filter(|worker| {
                    worker.count
                        > if self_count < expected {
                            // If we are underloaded (self < expected), we may steal from those workers.
                            expected
                        } else {
                            // Those are the overloaded ones, steal them if (self == expected).
                            expected + 1
                        }
                })
                .collect::<Vec<_>>()
                .choose(&mut rand::thread_rng())
                .cloned()
        } else {
            None
        };

        Ok(if let Some(victim_worker) = victim_worker {
            let query = doc! {
                "parent_uuid": victim_worker.id.to_string()
            };

            // TODO randomly pick one entry from victim
            info!("steal one entry");

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
                .await?
                .map_or(ScheduleResult::Conflict, ScheduleResult::Some)
        } else {
            // no need to steal anything
            ScheduleResult::None
        })
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send + Sync> DBOperation for ScheduleOp<T> {
    type Result = Option<(TaskInfo, T)>;

    async fn execute(&self, collection: &Collection) -> DBResult<Self::Result> {
        Ok(match self.mode {
            ScheduleMode::Auto => {
                if let Some(res) = self.do_acquire(collection).await? {
                    Some(res)
                } else {
                    loop {
                        match self.do_schedule_once(collection).await? {
                            ScheduleResult::Conflict => continue,
                            ScheduleResult::Some(res) => break Some(res),
                            ScheduleResult::None => break None,
                        }
                    }
                }
            }
            ScheduleMode::OutdatedOnly => self.do_acquire(collection).await?,
            ScheduleMode::StealOnly => match self.do_schedule_once(collection).await? {
                ScheduleResult::Some(res) => Some(res),
                _ => None,
            },
        }
        .map(|res| {
            info!("{:?}", res);
            (
                bson::from_document(res.clone()).unwrap(),
                bson::from_document(res).unwrap(),
            )
        }))
    }
}
