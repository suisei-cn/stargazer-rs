use actix::fut::ready;
use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, Context, ResponseActFuture, WrapFuture,
};
use actix_signal::SignalHandler;
use actix_web::{get, web, Responder};
use egg_mode::entities::MediaType;
use egg_mode::error::Result;
use egg_mode::user::UserID;
use egg_mode::{tweet, Token};
use mongodb::bson;
use serde::{Deserialize, Serialize};
use tracing::Span;
use tracing::{error, info, info_span, warn};
use tracing_actix::ActorInstrument;

use crate::db::{Coll, Collection, Document};
use crate::scheduler::messages::UpdateEntry;
use crate::scheduler::{Entry, Task, TaskInfo};
use crate::source::ToCollector;
use crate::utils::Scheduler;
use crate::ScheduleConfig;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct TwitterEntry {
    uid: u64,
    since: Option<u64>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct TwitterSince {
    since: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct Tweet {
    text: String,
    photos: Vec<String>,
    link: String,
    is_rt: bool,
}

#[derive(Debug, Clone, SignalHandler)]
pub struct TwitterActor {
    token: Token,
    entry: Entry<TwitterEntry>,
    schedule_config: ScheduleConfig,
    collection: Collection<Document>,
    info: TaskInfo,
    scheduler: Scheduler<Self>,
}

impl_task_field_getter!(TwitterActor, info, scheduler);
impl_stop_on_panic!(TwitterActor);
impl_to_collector_handler!(TwitterActor);

impl Actor for TwitterActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.span().in_scope(|| {
            info!("started");
        });

        ctx.run_interval(self.schedule_config.max_interval / 2, |act, ctx| {
            let token = act.token.clone();
            let entry = act.entry.data;
            ctx.spawn(
                fetch_tweets(token, entry)
                    .into_actor(act)
                    .then(|tweets, act, ctx| -> ResponseActFuture<Self, _> {
                        match tweets {
                            Ok((since, tweets)) => {
                                if !tweets.is_empty() {
                                    ctx.notify(ToCollector::new("twitter", tweets));
                                }
                                act.entry.data.since = since;
                                Box::pin(
                                    act.scheduler
                                        .send(UpdateEntry::new(act.info, TwitterSince { since }))
                                        .into_actor(act)
                                        .map(|res, _, _| Some(res)),
                                )
                            }
                            Err(e) => {
                                error!("tweet fetch error: {:?}", e);
                                ctx.stop();
                                Box::pin(ready(None).into_actor(act))
                            }
                        }
                    })
                    .map(|res, _, ctx| {
                        if let Some(res) = res {
                            if !res.unwrap_or(Ok(false)).unwrap_or(false) {
                                warn!("unable to renew ts, trying to stop");
                                ctx.stop();
                            }
                        }
                    })
                    .actor_instrument(act.span()),
            );
        });
    }
}

async fn fetch_tweets(token: Token, entry: TwitterEntry) -> Result<(Option<u64>, Vec<Tweet>)> {
    let tl = tweet::user_timeline(UserID::ID(entry.uid), false, true, &token);
    let (_, tweets) = tl.with_page_size(5).older(entry.since).await?;

    let new_since = tweets.first().map(|tweet| tweet.id).or(entry.since);

    Ok((
        new_since,
        tweets
            .response
            .into_iter()
            .map(|tweet| Tweet {
                text: tweet.text,
                photos: tweet
                    .entities
                    .media
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|medium| medium.media_type == MediaType::Photo)
                    .map(|medium| medium.media_url_https)
                    .collect(),
                link: format!(
                    "https://twitter.com/{}/status/{}",
                    tweet.user.unwrap().screen_name,
                    tweet.id
                ),
                is_rt: tweet.retweeted_status.is_some(),
            })
            .collect(),
    ))
}

impl Task for TwitterActor {
    const COLLECTION_NAME: &'static str = "twitter";
    type Entry = TwitterEntry;
    type Ctor = TwitterCtor;

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
            token: Token::Bearer(ctor.token),
            entry,
            schedule_config: ctor.schedule_config,
            collection,
            info,
            scheduler,
        }
    }

    fn span(&self) -> Span {
        let task_id = self.info.uuid;
        let uid = self.entry.data.uid;
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
        Self {
            schedule_config,
            token: token.to_string(),
        }
    }
}

pub struct TwitterColl;

#[get("/set")]
pub async fn set(
    coll: web::Data<Coll<TwitterColl>>,
    entry: web::Query<TwitterEntry>,
) -> impl Responder {
    coll.insert_one(&bson::to_document(&*entry).unwrap(), None)
        .await
        .unwrap();
    "ok"
}
