use actix::{Actor, Context, Handler, Message, Supervised, System, SystemService};
use actix_web::dev::ServerHandle;

#[derive(Debug, Clone, Message)]
#[rtype("()")]
pub struct RegisterHttpServer(pub ServerHandle);

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct Kill {
    pub graceful: bool,
}

#[derive(Debug, Default)]
pub struct KillerActor(Option<ServerHandle>);

impl KillerActor {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn kill(graceful: bool) {
        Self::from_registry().do_send(Kill { graceful });
    }
}

impl Actor for KillerActor {
    type Context = Context<Self>;
}

impl Supervised for KillerActor {}

impl SystemService for KillerActor {}

impl Handler<RegisterHttpServer> for KillerActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterHttpServer, _ctx: &mut Self::Context) -> Self::Result {
        self.0 = Some(msg.0);
    }
}

impl Handler<Kill> for KillerActor {
    type Result = ();

    fn handle(&mut self, msg: Kill, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(http_server) = &self.0 {
            drop(http_server.stop(msg.graceful)); // deliberately not awaited
        } else {
            System::current().stop();
        }
    }
}
