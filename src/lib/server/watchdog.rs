use std::cell::RefCell;

use actix::{Actor, Addr, Context};
use tokio::sync::mpsc::UnboundedSender;

thread_local! {
    static LOCAL_WATCHDOG: RefCell<Option<Addr<WatchdogActor>>> = RefCell::new(None);
}

#[derive(Debug)]
pub struct WatchdogActor(UnboundedSender<()>);

impl WatchdogActor {
    pub fn start(tx: UnboundedSender<()>) {
        let act = Self(tx);
        let addr = act.start();
        LOCAL_WATCHDOG.with(|f| *f.borrow_mut() = Some(addr));
    }
}

impl Actor for WatchdogActor {
    type Context = Context<Self>;
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.0.send(()).unwrap();
    }
}
