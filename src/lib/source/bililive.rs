use actix::fut::ready;
use actix::{Actor, ActorContext, ActorFutureExt, AsyncContext, Context, WrapFuture};
use bililive::tokio::connect_with_retry;
use bililive::{ConfigBuilder, RetryConfig};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{info, info_span, warn};
use tracing_actix::ActorInstrument;

use crate::db::{Collection, Document};
use crate::scheduler::messages::UpdateTimestamp;
use crate::scheduler::{Task, TaskInfo};
use crate::utils::Scheduler;
use crate::ScheduleConfig;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct BililiveEntry {
    uid: u64,
}

impl BililiveEntry {
    pub const fn uid(&self) -> u64 {
        self.uid
    }
}

#[derive(Debug, Clone)]
pub struct BililiveActor {
    uid: u64,
    schedule_config: ScheduleConfig,
    collection: Collection<Document>,
    info: TaskInfo,
    scheduler: Scheduler<Self>,
}

impl_task_field_getter!(BililiveActor, scheduler);
impl_stop_on_panic!(BililiveActor);
impl_message_target!(pub BililiveTarget, BililiveActor);

impl Actor for BililiveActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let task_id = self.info.uuid();
        let uid = self.uid;
        info_span!("bililive", ?task_id, uid).in_scope(|| {
            info!("started");
        });

        // update timestamp
        ctx.run_interval(self.schedule_config.max_interval() / 2, move |act, ctx| {
            ctx.spawn(
                act.scheduler
                    .send(UpdateTimestamp(act.info))
                    .into_actor(act)
                    .map(|res, _act, ctx| {
                        if !res.unwrap_or(Ok(false)).unwrap_or(false) {
                            warn!("unable to renew ts, trying to stop");
                            // op failed
                            ctx.stop();
                        }
                    })
                    .actor_instrument(info_span!("bililive", ?task_id, uid)),
            );
        });

        ctx.spawn(
            ready(())
                .into_actor(self)
                .then(move |_, act, _| {
                    async move {
                        connect_with_retry(
                            ConfigBuilder::new()
                                .by_uid(uid)
                                .await?
                                .fetch_conf()
                                .await?
                                .build()?,
                            RetryConfig::default(),
                        )
                        .await
                    }
                    .into_actor(act)
                })
                .map(|stream, _act, _| {
                    if let Err(ref e) = stream {
                        warn!("failed to connect stream: {}", e);
                    }
                    stream
                })
                .then(move |stream, act, _| {
                    async move {
                        if let Ok(mut stream) = stream {
                            // poll packet
                            while let Some(msg) = stream.next().await {
                                match msg {
                                    Ok(msg) => {
                                        // TODO output
                                        info!("{:?}", msg);
                                    }
                                    Err(e) => {
                                        warn!("stream error: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    .into_actor(act)
                })
                .map(|_, _, ctx| ctx.stop())
                .actor_instrument(info_span!("bililive", ?task_id, uid)),
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
        entry: Self::Entry,
        ctor: Self::Ctor,
        scheduler: Scheduler<Self>,
        info: TaskInfo,
        collection: Collection<Document>,
    ) -> Self {
        Self {
            uid: entry.uid(),
            schedule_config: ctor,
            collection,
            info,
            scheduler,
        }
    }
}
