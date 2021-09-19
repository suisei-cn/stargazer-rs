use std::cell::Cell;

use actix::{Actor, Addr, Context};
use tokio::sync::mpsc::UnboundedSender;

thread_local! {
    static LOCAL_WATCHDOG: Cell<Option<Addr<WatchdogActor>>> = Cell::new(None);
}

#[derive(Debug)]
pub struct WatchdogActor(UnboundedSender<()>);

impl WatchdogActor {
    /// Start `WatchdogActor` on the current arbiter with given signal sender.
    ///
    /// # Panics
    /// Panics when there's already a `WatchdogActor` on the same arbiter.
    pub fn start(tx: UnboundedSender<()>) {
        let act = Self(tx);
        let addr = act.start();
        LOCAL_WATCHDOG.with(|f| {
            assert!(
                f.replace(Some(addr)).is_none(),
                "cannot run two watchdogs on the same arbiter"
            );
        });
    }
}

impl Actor for WatchdogActor {
    type Context = Context<Self>;
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        let _ = self.0.send(());
    }
}
