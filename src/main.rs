use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use actix::{Actor, AsyncContext, Context, Handler, Message};
use actix_web::Responder;
use clap::{AppSettings, Clap};
use uuid::Uuid;

use stargazer_lib::{
    impl_message_target, impl_stop_on_panic, ArbiterContext, Config, KillerActor, Server,
    ServerMode,
};

mod app;

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

struct PanicActor {
    instance_id: Uuid,
    arbiter_id: Uuid,
    timed_panic: bool,
}

impl_message_target!(PanicTarget, PanicActor);

impl Actor for PanicActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        if self.timed_panic {
            ctx.run_later(Duration::from_secs(5), |act, ctx| {
                panic!("boom");
            });
        }
    }
}

#[derive(Message)]
#[rtype("()")]
struct PanicSignal;

impl Handler<PanicSignal> for PanicActor {
    type Result = ();

    fn handle(&mut self, msg: PanicSignal, ctx: &mut Self::Context) -> Self::Result {
        println!("inst {} arb {}", self.instance_id, self.arbiter_id);
        panic!("inst {} arb {}", self.instance_id, self.arbiter_id)
    }
}

impl_stop_on_panic!(PanicActor);

#[actix_web::get("/panic")]
async fn panic_serv(ctx: actix_web::web::Data<ArbiterContext>) -> impl Responder {
    ctx.send(PanicTarget, PanicSignal).unwrap().await;
    "ok"
}

#[actix_web::main]
async fn main() {
    pretty_env_logger::init();
    let opts = Opts::parse();
    let config = Config::new(opts.config.as_deref());
    let lock_once = Arc::new(AtomicBool::new(false));
    Server::new(move |instance_id| {
        let ctx = ArbiterContext::new(instance_id);

        let panic_act = PanicActor {
            instance_id,
            arbiter_id: ctx.arbiter_id(),
            timed_panic: !lock_once.fetch_or(true, Ordering::SeqCst),
        };
        let panic_addr = panic_act.start();

        // register actor addrs
        (ctx.register_addr(panic_addr), |config| {
            config.service(panic_serv);
        })
    })
    // .run(ServerMode::HTTP {
    //     port: "127.0.0.1:8080".parse().unwrap(),
    // })
    .run(ServerMode::NoHTTP)
    .unwrap()
    .await;
    println!("{:?}", config);
}
