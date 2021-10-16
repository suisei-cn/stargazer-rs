use actix::{Actor, ActorContext, ActorFutureExt, AsyncContext, Context, WrapFuture};
use actix_signal::SignalHandler;
use actix_web::{get, web, Responder};
use mongodb::bson;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, info_span, warn, Span};
use tracing_actix::ActorInstrument;

use crate::db::{Coll, Collection, Document};
use crate::scheduler::messages::UpdateEntry;
use crate::scheduler::{InfoGetter, SchedulerGetter, Task, TaskInfo};
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
impl_message_target!(pub DebugTarget, DebugActor);
impl_to_collector_handler!(DebugActor);

impl Actor for DebugActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("started");
        });

        ctx.run_interval(self.schedule_config.max_interval / 2, |act, ctx| {
            ctx.spawn(
                act.get_scheduler()
                    .send(UpdateEntry::empty_payload(act.get_info()))
                    .into_actor(act)
                    .map(|res, _act, ctx| {
                        if !res.unwrap_or(Ok(false)).unwrap_or(false) {
                            warn!("unable to renew ts, trying to stop");
                            ctx.stop();
                        }
                    }),
            )
            .actor_instrument(act.span());
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("stopped");
        });
    }
}

impl Task for DebugActor {
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
