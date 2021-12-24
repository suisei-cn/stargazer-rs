use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::num::ParseIntError;
use std::str::FromStr;

use actix::fut::ready;
use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, Context, StreamHandler, WrapFuture,
};
use actix_bililive::errors::StreamError;
use actix_bililive::{connect_with_retry, ConfigBuilder, Packet, RetryConfig};
use actix_signal::SignalHandler;
use actix_web::{get, web, Responder};
use hmap_serde::Labelled;
use mongodb::bson;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, info_span, Span};
use tracing_actix::ActorInstrument;

use crate::db::{Coll, Collection, Document};
use crate::scheduler::{Entry, Task, TaskInfo};
use crate::source::ToCollector;
use crate::utils::Scheduler;
use crate::ScheduleConfig;

type BoxedError = Box<dyn Error>;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct BililiveEntry {
    pub uid: u64,
}

impl Labelled for BililiveEntry {
    const KEY: &'static str = "bililive";
}

impl FromStr for BililiveEntry {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            uid: u64::from_str(s)?,
        })
    }
}

impl Display for BililiveEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.uid)
    }
}

#[derive(Debug, Clone, SignalHandler)]
pub struct BililiveActor {
    entry: Entry<BililiveEntry>,
    schedule_config: ScheduleConfig,
    collection: Collection<Document>,
    info: TaskInfo,
    scheduler: Scheduler<Self>,
}

impl_task_field_getter!(BililiveActor, info, scheduler);
impl_stop_on_panic!(BililiveActor);
impl_to_collector_handler!(BililiveActor, entry);

impl StreamHandler<Result<Packet, StreamError>> for BililiveActor {
    fn handle(&mut self, item: Result<Packet, StreamError>, ctx: &mut Self::Context) {
        let _span = self.span().entered();
        match item {
            Ok(msg) => {
                if let Ok(msg) = msg.json::<serde_json::Value>() {
                    debug!("publishing event to collector");
                    ctx.notify(ToCollector::new("bililive", msg));
                }
            }
            Err(e) => {
                error!("stream error: {}", e);
                ctx.stop();
            }
        }
    }
}

impl Actor for BililiveActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("started");
        });

        let uid = self.entry.data.uid;
        ctx.spawn(
            ready(())
                .into_actor(self)
                .then(move |_, act, _| {
                    async move {
                        connect_with_retry(
                            ConfigBuilder::new()
                                .by_uid(uid)
                                .await
                                .map_err(|e| Box::new(e) as BoxedError)?
                                .fetch_conf()
                                .await
                                .map_err(|e| Box::new(e) as BoxedError)?
                                .build(),
                            RetryConfig::default(),
                        )
                        .await
                        .map_err(|e| Box::new(e) as BoxedError)
                    }
                    .into_actor(act)
                })
                .map(|stream, _, ctx| match stream {
                    Ok(stream) => {
                        info!("stream added");
                        Self::add_stream(stream, ctx);
                    }
                    Err(e) => {
                        error!("failed to connect stream: {}", e);
                        ctx.stop();
                    }
                })
                .actor_instrument(self.span()),
        );
    }
}

impl Task for BililiveActor {
    type Entry = BililiveEntry;
    type Ctor = ScheduleConfig;

    fn query() -> Document {
        Document::new()
    }

    fn construct(
        entry: Entry<Self::Entry>,
        ctor: Self::Ctor,
        scheduler: Scheduler<Self>,
        info: TaskInfo,
        collection: Collection<Document>,
    ) -> Self {
        Self {
            entry,
            schedule_config: ctor,
            collection,
            info,
            scheduler,
        }
    }

    fn span(&self) -> Span {
        let task_id = self.info.uuid;
        let uid = self.entry.data.uid;
        info_span!("bililive", ?task_id, uid)
    }
}

pub struct BililiveColl;

#[get("/set")]
pub async fn set(
    coll: web::Data<Coll<BililiveColl>>,
    entry: web::Query<BililiveEntry>,
) -> impl Responder {
    debug!("writing to db");
    coll.insert_one(&bson::to_document(&*entry).unwrap(), None)
        .await
        .unwrap();
    debug!("writing to db ok");
    "ok"
}
