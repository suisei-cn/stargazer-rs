use actix::{Actor, ActorContext, AsyncContext, Context};
use egg_mode::tweet::TimelineFuture;
use egg_mode::user::UserID;
use egg_mode::{tweet, Token};
use serde::{Deserialize, Serialize};
use tracing::Span;
use tracing::{error, info, info_span};

use crate::db::{Collection, Document};
use crate::scheduler::{Task, TaskInfo};
use crate::utils::Scheduler;
use crate::ScheduleConfig;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct TwitterEntry {
    uid: u64,
    since: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TwitterActor {
    token: Token,
    entry: TwitterEntry,
    schedule_config: ScheduleConfig,
    collection: Collection<Document>,
    info: TaskInfo,
    scheduler: Scheduler<Self>,
}

impl_task_field_getter!(TwitterActor, info, scheduler);
impl_stop_on_panic!(TwitterActor);
impl_message_target!(pub TwitterTarget, TwitterActor);
impl_tick_handler!(TwitterActor);
impl_to_collector_handler!(TwitterActor);

impl Actor for TwitterActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("started");
        });

        ctx.run_interval(self.schedule_config.max_interval() / 2, |act, ctx| {
            let token = act.token.clone();
            let entry = act.entry;
            ctx.spawn(
                fetch_tweets(&token, entry)
                    .into_actor(act)
                    .map(|tweets, _, ctx| {
                        match tweets {
                            Ok((_, tweets)) => {
                                // TODO ToCollector
                                info!("{:?}", tweets);
                            }
                            Err(e) => {
                                error!("tweet fetch error: {:?}", e);
                                ctx.stop();
                            }
                        }
                    })
                    .actor_instrument(act.span()),
            );
        });
    }
}

fn fetch_tweets(token: &Token, entry: TwitterEntry) -> TimelineFuture {
    let tl = tweet::user_timeline(UserID::ID(entry.uid), false, true, token);
    tl.with_page_size(5).older(entry.since)
}

impl Task for TwitterActor {
    type Entry = TwitterEntry;
    type Ctor = TwitterCtor;

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
            token: Token::Bearer(ctor.token),
            entry,
            schedule_config: ctor.schedule_config,
            collection,
            info,
            scheduler,
        }
    }

    fn span(&self) -> Span {
        let task_id = self.info.uuid();
        let uid = self.entry.uid;
        info_span!("twitter", ?task_id, uid)
    }
}

#[derive(Debug, Clone)]
pub struct TwitterCtor {
    schedule_config: ScheduleConfig,
    token: String,
}

impl TwitterCtor {
    pub fn new(schedule_config: ScheduleConfig, token: &str) -> Self {
        TwitterCtor {
            schedule_config,
            token: token.to_string(),
        }
    }
}
