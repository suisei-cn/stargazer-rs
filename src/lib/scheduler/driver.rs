use std::mem;
use std::time::Duration;

use actix::{Actor, Addr, AsyncContext, AtomicResponse, Context, Handler, Message, WrapFuture};
use rand::{thread_rng, Rng};
use tracing::info_span;
use tracing_actix::ActorInstrument;

use crate::scheduler::messages::TrySchedule;
use crate::scheduler::ops::ScheduleMode;
use crate::scheduler::{ScheduleActor, Task};
use crate::ScheduleConfig;

#[derive(Debug)]
pub struct ScheduleDriverActor<T: Task> {
    config: ScheduleConfig,
    addrs: Vec<Addr<ScheduleActor<T>>>,
}

impl_stop_on_panic!(ScheduleDriverActor<T: Task>);

impl<T: Task> ScheduleDriverActor<T> {
    pub fn new(config: ScheduleConfig) -> Self {
        Self {
            config,
            addrs: Default::default(),
        }
    }
}

#[derive(Debug, Copy, Clone, Message)]
#[rtype("()")]
pub struct ScheduleAll(ScheduleMode);

#[derive(Debug, Clone, Message)]
#[rtype("()")]
pub struct RegisterScheduler<T: Task>(Addr<ScheduleActor<T>>);

impl<T: Task> RegisterScheduler<T> {
    pub fn new(addr: Addr<ScheduleActor<T>>) -> Self {
        Self(addr)
    }
}

impl<T: Task> Actor for ScheduleDriverActor<T> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut skip_once = true;
        ctx.run_interval(self.config.schedule_interval(), |_, ctx| {
            let delay: u64 = thread_rng().gen_range(0..1000);
            ctx.notify_later(
                ScheduleAll(ScheduleMode::OutdatedOnly),
                Duration::from_millis(delay),
            );
        });
        ctx.run_interval(self.config.balance_interval(), move |_, ctx| {
            if skip_once {
                skip_once = false;
                return;
            }
            let delay: u64 = thread_rng().gen_range(0..1000);
            ctx.notify_later(
                ScheduleAll(ScheduleMode::StealOnly),
                Duration::from_millis(delay),
            );
        });
    }
}

impl<T> Handler<ScheduleAll> for ScheduleDriverActor<T>
where
    T: Task,
    ScheduleActor<T>: Actor<Context = Context<ScheduleActor<T>>>,
{
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, msg: ScheduleAll, _ctx: &mut Self::Context) -> Self::Result {
        let mut addrs = self.addrs.clone();
        AtomicResponse::new(Box::pin(
            async move {
                while !addrs.is_empty() {
                    addrs = schedule_one_round(mem::take(&mut addrs), msg.0).await;
                }
            }
            .into_actor(self)
            .actor_instrument(info_span!("schedule_driver")),
        ))
    }
}

async fn schedule_one_round<T>(
    addrs: Vec<Addr<ScheduleActor<T>>>,
    mode: ScheduleMode,
) -> Vec<Addr<ScheduleActor<T>>>
where
    T: Task,
    ScheduleActor<T>: Actor<Context = Context<ScheduleActor<T>>>,
{
    let mut new_addrs = vec![];
    for addr in addrs {
        let resp = addr
            .send(TrySchedule::new(mode))
            .await
            .expect("unable to send msg to scheduler");
        if matches!(resp, Ok(Some(_))) {
            new_addrs.push(addr);
        }
    }
    new_addrs
}

impl<T> Handler<RegisterScheduler<T>> for ScheduleDriverActor<T>
where
    T: Task,
{
    type Result = ();

    fn handle(&mut self, msg: RegisterScheduler<T>, _ctx: &mut Self::Context) -> Self::Result {
        self.addrs.push(msg.0);
    }
}
