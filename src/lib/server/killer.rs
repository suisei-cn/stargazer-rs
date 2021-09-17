use std::cell::Cell;

use actix::{Actor, Addr, Context, Handler, Message, System};
use actix_web::dev::Server as ActixServer;

thread_local! {
    static LOCAL_KILLER: Cell<Option<Addr<KillerActor>>> = Cell::new(None);
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct Kill {
    graceful: bool,
}

impl Kill {
    pub const fn new(graceful: bool) -> Self {
        Self { graceful }
    }
}

#[derive(Clone, Debug, Default)]
pub struct KillerActor(Option<ActixServer>);

impl KillerActor {
    pub fn start(srv: Option<ActixServer>) {
        let act = Self(srv);
        let addr = act.start();
        LOCAL_KILLER.with(|f| {
            assert!(
                f.replace(Some(addr)).is_none(),
                "cannot run two killers on the same arbiter"
            );
        });
    }

    pub fn kill(graceful: bool) {
        LOCAL_KILLER.with(|f| {
            if let Some(killer) = f.replace(None) {
                killer.do_send(Kill::new(graceful))
            }
        });
    }
}

impl Actor for KillerActor {
    type Context = Context<Self>;
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
