use std::path::PathBuf;

use actix::Actor;
use clap::{AppSettings, Clap};

use stargazer_lib::db::{connect_db, Collection, Document};
use stargazer_lib::scheduler::ScheduleActor;
use stargazer_lib::source::bililive::BililiveActor;
use stargazer_lib::collector::CollectorActor;
use stargazer_lib::{ArbiterContext, Config, ScheduleConfig, Server, AMQP};
use stargazer_lib::collector::amqp::AMQPFactory;

#[derive(Clap)]
#[clap(
    version = "1.0",
    author = "LightQuantum <self@lightquantum.me> and George Miao <gm@miao.dev>"
)]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Sets a custom config file. This flag overrides system-wide and user-wide configs.
    #[clap(short, long)]
    config: Option<PathBuf>,
}

#[actix_web::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();
    let config = Config::new(opts.config.as_deref()).unwrap();
    let amqp_config = config.amqp().clone();

    let db = connect_db(config.mongodb().uri(), config.mongodb().database())
        .await
        .expect("unable to connect to db");
    let coll: Collection<Document> = db.collection("bililive");

    Server::new(move |instance_id| {
        let ctx = ArbiterContext::new(instance_id);

        let bililive_actor: ScheduleActor<BililiveActor> = ScheduleActor::builder()
            .collection(coll.clone())
            .ctor_builder(ScheduleConfig::default)
            .config(ScheduleConfig::default())
            .build();
        let bililive_addr = bililive_actor.start();

        let mut collector_factories = Vec::new();
        if let AMQP::Enabled { uri, exchange } = amqp_config.clone() {
            collector_factories.push(AMQPFactory::new(uri.as_str(), exchange.as_str()).into());
        }
        let collector_actor = CollectorActor::new(collector_factories);
        let collector_addr = collector_actor.start();

        // register actor addrs
        (ctx.register_addr(bililive_addr).register_addr(collector_addr), |_| {})
    })
    .run(config.http().into())
    .unwrap()
    .await
    .unwrap();
}
