use std::fmt::Debug;

use actix::fut::ready;
use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, Context, Handler, Message,
    ResponseActFuture, StreamHandler, WrapFuture,
};
use bililive::tokio::connect_with_retry;
use bililive::{BililiveError, ConfigBuilder, Packet, RetryConfig};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, info_span, warn, Span};
use tracing_actix::ActorInstrument;

use crate::collector::{CollectorTarget, Publish};
use crate::db::{Collection, Document};
use crate::request::RequestTrait;
use crate::scheduler::{Task, TaskInfo, Tick, TickOrStop};
use crate::utils::Scheduler;
use crate::{ArbiterContext, ScheduleConfig};

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

impl_task_field_getter!(BililiveActor, info, scheduler);
impl_stop_on_panic!(BililiveActor);
impl_message_target!(pub BililiveTarget, BililiveActor);
impl_tick_handler!(BililiveActor);

#[derive(Debug, Clone, Message)]
#[rtype("()")]
struct ToCollector<T: Serialize>(T);

impl<T: 'static + Serialize + Send + Sync> Handler<ToCollector<T>> for BililiveActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: ToCollector<T>, ctx: &mut Self::Context) -> Self::Result {
        Box::pin(
            ctx.address()
                .send(Tick)
                .into_actor(self)
                .map(move |res, _act, ctx| {
                    let holding_ownership = res.unwrap_or(false);
                    if holding_ownership {
                        ArbiterContext::with(|ctx| {
                            ctx.send(CollectorTarget, Publish::new("bililive", msg.0))
                                .unwrap()
                                .immediately();
                        });
                    } else {
                        warn!("unable to renew ts, trying to stop");
                        ctx.stop();
                    };
                })
                .actor_instrument(self.span()),
        )
    }
}

impl StreamHandler<Result<Packet, BililiveError>> for BililiveActor {
    fn handle(&mut self, item: Result<Packet, BililiveError>, ctx: &mut Self::Context) {
        let _span = self.span().entered();
        match item {
            Ok(msg) => {
                if let Ok(msg) = msg.json::<serde_json::Value>() {
                    debug!("publishing event to collector");
                    ctx.notify(ToCollector(msg));
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

        // update timestamp
        // TODO may not wake on time, investigation needed
        ctx.run_interval(self.schedule_config.max_interval() / 2, |_, ctx| {
            ctx.notify(TickOrStop);
        });

        let uid = self.uid;
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
                .map(|stream, _, ctx| match stream {
                    Ok(stream) => {
                        info!("stream added");
                        Self::add_stream(stream, ctx);
                    }
                    Err(e) => warn!("failed to connect stream: {}", e),
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

    fn span(&self) -> Span {
        let task_id = self.info.uuid();
        let uid = self.uid;
        info_span!("bililive", ?task_id, uid)
    }
}
