use std::ops::Deref;
use std::time::{SystemTime, UNIX_EPOCH};

use actix::Addr;
use actix_rt::task::JoinHandle;

use crate::scheduler::actor::ScheduleActor;

pub type Scheduler<T> = Addr<ScheduleActor<T>>;

pub trait TypeEq {
    type Other;
}

impl<T> TypeEq for T {
    type Other = Self;
}

#[macro_export]
/// impl_message_target!((pub) ActorTarget, Actor)
macro_rules! impl_message_target {
    ($name: ident, $target_ty: ty) => {
        #[derive(Debug, Copy, Clone)]
        struct $name;
        impl $crate::context::MessageTarget for $name {
            type Actor = $target_ty;
            type Addr = actix::Addr<$target_ty>;
        }
    };
    (pub $name: ident, $target_ty: ty) => {
        #[derive(Debug, Copy, Clone)]
        pub struct $name;
        impl $crate::context::MessageTarget for $name {
            type Actor = $target_ty;
            type Addr = actix::Addr<$target_ty>;
        }
    };
}

#[macro_export]
/// impl_stop_on_panic!(Actor)
macro_rules! impl_stop_on_panic {
    ($name: ident) => {
        impl Drop for $name {
            fn drop(&mut self) {
                if std::thread::panicking() {
                    crate::KillerActor::kill(true)
                }
            }
        }
    };
}

#[macro_export]
/// impl_task_field_getter!(Actor, scheduler)
macro_rules! impl_task_field_getter {
    ($self: ident, $info: ident, $scheduler: ident) => {
        impl $crate::scheduler::InfoGetter for $self {
            fn get_info(&self) -> $crate::scheduler::TaskInfo {
                self.$info
            }
        }
        impl $crate::scheduler::SchedulerGetter for $self {
            fn get_scheduler(&self) -> &actix::Addr<$crate::scheduler::actor::ScheduleActor<Self>> {
                &self.$scheduler
            }
        }
    };
}

#[macro_export]
macro_rules! impl_to_collector_handler {
    ($Self: ident) => {
        impl<T: 'static + serde::Serialize + Send + Sync>
            actix::Handler<crate::source::ToCollector<T>> for $Self
        {
            type Result = actix::ResponseActFuture<Self, ()>;

            fn handle(
                &mut self,
                msg: crate::source::ToCollector<T>,
                _ctx: &mut Self::Context,
            ) -> Self::Result {
                use actix::ActorFutureExt;
                use actix::WrapFuture;
                use tracing_actix::ActorInstrument;

                use crate::request::RequestTrait;
                use crate::scheduler::InfoGetter;
                use crate::scheduler::SchedulerGetter;

                Box::pin(
                    self.get_scheduler()
                        .send(crate::scheduler::messages::UpdateEntry::empty_payload(
                            self.get_info(),
                        ))
                        .into_actor(self)
                        .map(move |res, _act, ctx| {
                            let holding_ownership = res.unwrap_or(Ok(false)).unwrap_or(false);
                            if holding_ownership {
                                crate::context::ArbiterContext::with(|ctx| {
                                    ctx.send(
                                        crate::collector::CollectorTarget,
                                        crate::collector::Publish::new(&*msg.topic, msg.body),
                                    )
                                    .unwrap()
                                    .immediately();
                                });
                            } else {
                                tracing::warn!("unable to renew ts, trying to stop");
                                ctx.stop();
                            };
                        })
                        .actor_instrument(self.span()),
                )
            }
        }
    };
}

#[allow(clippy::cast_possible_truncation)]
pub fn timestamp(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

pub struct CancelOnDrop<T> {
    handle: JoinHandle<T>,
}

impl<T> CancelOnDrop<T> {
    pub fn new(handle: JoinHandle<T>) -> Self {
        Self { handle }
    }
}

impl<T> Deref for CancelOnDrop<T> {
    type Target = JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl<T> Drop for CancelOnDrop<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct CustomGuard<T>
where
    T: FnMut(),
{
    on_exit: T,
}

impl<T> CustomGuard<T>
where
    T: FnMut(),
{
    pub fn new(on_exit: T) -> Self {
        Self { on_exit }
    }
}

impl<T> Drop for CustomGuard<T>
where
    T: FnMut(),
{
    fn drop(&mut self) {
        (self.on_exit)();
    }
}
