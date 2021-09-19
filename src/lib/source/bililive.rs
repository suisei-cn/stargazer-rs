use actix::fut::ready;
use actix::{Actor, ActorContext, ActorFutureExt, AsyncContext, Context, WrapFuture};
use bililive::tokio::connect_with_retry;
use bililive::{ConfigBuilder, RetryConfig};
use futures::StreamExt;
use log::{info, warn};
use serde::{Deserialize, Serialize};

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
    collection: Collection,
    info: TaskInfo,
    scheduler: Scheduler<Self>,
}

impl_task_field_getter!(BililiveActor, scheduler);
impl_stop_on_panic!(BililiveActor);
impl_message_target!(pub BililiveTarget, BililiveActor);

impl Actor for BililiveActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            "BililiveActor {} started, handling {}",
            self.info.uuid(),
            self.uid
        );

        // update timestamp
        ctx.run_interval(self.schedule_config.max_interval() / 2, |act, ctx| {
            ctx.spawn(
                act.scheduler
                    .send(UpdateTimestamp(act.info))
                    .into_actor(act)
                    .map(|res, act, ctx| {
                        if !res.unwrap_or(Ok(false)).unwrap_or(false) {
                            warn!(
                                "BililiveActor {} unable to renew ts, trying to stop",
                                act.info.uuid()
                            );
                            // op failed
                            ctx.stop();
                        }
                    }),
            );
        });

        let uid = self.uid;
        ctx.spawn(
            ready(())
                .into_actor(self)
                .then(move |_, this, _| {
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
                    .into_actor(this)
                })
                .map(|stream, this, _| {
                    if let Err(ref e) = stream {
                        warn!(
                            "BililiveActor {} failed to connect stream: {}",
                            this.info.uuid(),
                            e
                        );
                    }
                    stream
                })
                .then(move |stream, this, _| {
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
                                        warn!("Bililive {} error: {}", uid, e);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    .into_actor(this)
                })
                .map(|_, _, ctx| ctx.stop()),
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
        collection: Collection,
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
