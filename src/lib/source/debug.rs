use actix::{Actor, Context};
use actix_signal::SignalHandler;
use actix_web::{get, web, Responder};
use mongodb::bson;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, info_span, Span};

use crate::db::{Coll, Collection, Document};
use crate::scheduler::{Task, TaskInfo};
use crate::utils::Scheduler;
use crate::ScheduleConfig;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct DebugEntry {
    id: u64,
}

#[derive(Debug, Clone, SignalHandler)]
pub struct DebugActor {
    entry: DebugEntry,
    schedule_config: ScheduleConfig,
    info: TaskInfo,
    scheduler: Scheduler<Self>,
}
impl_task_field_getter!(DebugActor, info, scheduler);
impl_stop_on_panic!(DebugActor);
impl_to_collector_handler!(DebugActor);

impl Actor for DebugActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("started");
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("stopped");
        });
    }
}

impl Task for DebugActor {
    const COLLECTION_NAME: &'static str = "debug";
    type Entry = DebugEntry;
    type Ctor = ScheduleConfig;

    fn query() -> Document {
        Document::new()
    }

    fn construct(
        entry: Self::Entry,
        ctor: Self::Ctor,
        scheduler: Scheduler<Self>,
        info: TaskInfo,
        _collection: Collection<Document>,
    ) -> Self {
        Self {
            entry,
            schedule_config: ctor,
            info,
            scheduler,
        }
    }

    fn span(&self) -> Span {
        let task_id = self.info.uuid;
        let entry_id = self.entry.id;
        info_span!("debug", ?task_id, entry_id)
    }
}

pub struct DebugColl;

#[get("/set")]
pub async fn set(
    coll: web::Data<Coll<DebugColl>>,
    entry: web::Query<DebugEntry>,
) -> impl Responder {
    debug!("writing to db");
    coll.insert_one(&bson::to_document(&*entry).unwrap(), None)
        .await
        .unwrap();
    debug!("writing to db ok");
    "ok"
}
