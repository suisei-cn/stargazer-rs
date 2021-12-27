use std::path::PathBuf;
use std::sync::Arc;

use actix::fut::ready;
use actix::Actor;
use actix_web::web::Data;
use actix_web::{get, web, Responder};
use clap::Parser;
use itertools::Itertools;

use stargazer_lib::collector::amqp::AMQPFactory;
use stargazer_lib::collector::debug::DebugCollectorFactory;
use stargazer_lib::collector::CollectorActor;
use stargazer_lib::db::{connect_db, Coll, Collection, Document};
use stargazer_lib::manager::{Manager, Vtuber};
use stargazer_lib::o;
use stargazer_lib::scheduler::driver::ScheduleDriverActor;
use stargazer_lib::scheduler::messages::{ActorsIter, UpdateAll};
use stargazer_lib::scheduler::ScheduleActor;
use stargazer_lib::source::bililive::{BililiveActor, BililiveColl};
use stargazer_lib::source::debug::{DebugActor, DebugColl};
use stargazer_lib::source::twitter::{TwitterActor, TwitterColl, TwitterCtor};
use stargazer_lib::{
    ArbiterContext, Config, InstanceContext, ScheduleConfig, Server, TwitterConfig, AMQP,
};

#[derive(Parser)]
#[clap(
    version = "1.0",
    author = "LightQuantum <self@lightquantum.me> and George Miao <gm@miao.dev>"
)]
struct Opts {
    /// Sets a custom config file. This flag overrides system-wide and user-wide configs.
    #[clap(short, long)]
    config: Option<PathBuf>,
}

#[get("/status")]
async fn status(ctx: web::Data<InstanceContext>) -> impl Responder {
    // evict outdated tasks
    ctx.send::<ScheduleActor<BililiveActor>, _>(&UpdateAll { evict: true })
        .unwrap()
        .await
        .unwrap();

    let resp = ctx
        .send::<ScheduleActor<BililiveActor>, _>(&ActorsIter::new(|map| Box::pin(ready(map.len()))))
        .unwrap()
        .await
        .unwrap()
        .into_iter()
        .sorted_by_key(|i| i.0)
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect_vec();

    format!("{:#?}", resp)
}

#[actix_web::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();
    let config = Config::new(opts.config.as_deref()).unwrap();
    let collector_config = config.collector.clone();
    let sched_config = ScheduleConfig::default();

    let source_config = config.source.clone();
    let twitter_config = source_config.twitter.clone();
    let bililive_config = source_config.bililive;
    let debug_source_config = source_config.debug;

    let database = connect_db(config.mongodb.uri(), config.mongodb.database())
        .await
        .expect("unable to connect to db");

    // TODO --- remove this part of code cuz they will be replaced by central metadata actor
    let coll_bililive: Collection<Document> = database.collection("bililive");
    let coll_twitter: Collection<Document> = database.collection("twitter");
    let coll_debug: Collection<Document> = database.collection("debug");

    let coll_vtuber: Collection<Vtuber> = database.collection("vtuber");

    let arc_coll_bililive: Arc<Coll<BililiveColl>> = Arc::new(Coll::new(coll_bililive.clone()));
    let arc_coll_twitter: Arc<Coll<TwitterColl>> = Arc::new(Coll::new(coll_twitter.clone()));
    let arc_coll_debug: Arc<Coll<DebugColl>> = Arc::new(Coll::new(coll_debug.clone()));
    // TODO ---

    let bililive_driver = ScheduleDriverActor::new(sched_config).start();
    let twitter_driver = ScheduleDriverActor::new(sched_config).start();
    let debug_driver = ScheduleDriverActor::new(sched_config).start();
    Server::new(move |instance_id| {
        let database = database.clone();
        let coll_vtuber = coll_vtuber.clone();

        let collector_config = collector_config.clone();
        let ctx = ArbiterContext::new(instance_id);

        let bililive_actor: Option<ScheduleActor<BililiveActor>> = if bililive_config.enabled {
            Some(
                ScheduleActor::builder()
                    .db(&database)
                    .ctor_builder(||())
                    .config(sched_config)
                    .driver(bililive_driver.clone())
                    .build(),
            )
        } else {
            None
        };

        let twitter_actor: Option<ScheduleActor<TwitterActor>> =
            if let TwitterConfig::Enabled { token } = &twitter_config {
                let token = token.clone();
                Some(
                    ScheduleActor::builder()
                        .db(&database)
                        .ctor_builder(move || TwitterCtor::new(sched_config, &*token))
                        .config(sched_config)
                        .driver(twitter_driver.clone())
                        .build(),
                )
            } else {
                None
            };

        let debug_actor: Option<ScheduleActor<DebugActor>> = if debug_source_config.enabled {
            Some(
                ScheduleActor::builder()
                    .db(&database)
                    .ctor_builder(|| ())
                    .config(sched_config)
                    .driver(debug_driver.clone())
                    .build(),
            )
        } else {
            None
        };

        let bililive_addr = bililive_actor.map(Actor::start);
        let twitter_addr = twitter_actor.map(Actor::start);
        let debug_addr = debug_actor.map(Actor::start);

        let ctx = o!(bililive_addr.map_or(ctx, |addr| ctx.register_addr(addr)));
        let ctx = o!(twitter_addr.map_or(ctx, |addr| ctx.register_addr(addr)));
        let ctx = o!(debug_addr.map_or(ctx, |addr| ctx.register_addr(addr)));

        let mut collector_factories = Vec::new();
        if let AMQP::Enabled { uri, exchange } = collector_config.amqp {
            collector_factories.push(AMQPFactory::new(uri.as_str(), exchange.as_str()).into());
        }
        if collector_config.debug.enabled {
            collector_factories.push(DebugCollectorFactory.into());
        }
        let collector_actor = CollectorActor::new(database.clone(), collector_factories);
        let collector_addr = collector_actor.start();

        let arc_coll_bililive = arc_coll_bililive.clone();
        let arc_coll_twitter = arc_coll_twitter.clone();
        let arc_coll_debug = arc_coll_debug.clone();

        let manager = Manager::new(database, coll_vtuber)
            .register::<BililiveActor>()
            .register::<TwitterActor>()
            .register::<DebugActor>();

        // register actor addrs
        (ctx.register_addr(collector_addr), move |cfg| {
            cfg.app_data(Data::from(arc_coll_bililive))
                .app_data(Data::from(arc_coll_twitter))
                .app_data(Data::from(arc_coll_debug))
                .service(status)
                .service(web::scope("/bililive").service(stargazer_lib::source::bililive::set))
                .service(web::scope("/twitter").service(stargazer_lib::source::twitter::set))
                .service(web::scope("/debug").service(stargazer_lib::source::debug::set))
                .service(manager.build("/manage"));
        })
    })
    .workers(config.basic.workers)
    .run(config.http.into())
    .unwrap()
    .await
    .unwrap();
}
