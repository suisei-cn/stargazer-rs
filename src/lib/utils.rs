use std::time::{SystemTime, UNIX_EPOCH};

use actix::Addr;

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
macro_rules! impl_tick_handler {
    ($Self: ident) => {
        #[allow(unused_imports)]
        use actix::ActorFutureExt as _ActorFutureExt;
        #[allow(unused_imports)]
        use actix::WrapFuture as _WrapFuture;
        #[allow(unused_imports)]
        use tracing_actix::ActorInstrument as _ActorInstrument;
        impl actix::Handler<$crate::scheduler::Tick> for $Self {
            type Result = actix::ResponseActFuture<Self, bool>;

            fn handle(
                &mut self,
                _msg: $crate::scheduler::Tick,
                _ctx: &mut Self::Context,
            ) -> Self::Result {
                $crate::scheduler::handle_tick(self)
            }
        }
        impl actix::Handler<$crate::scheduler::TickOrStop> for $Self {
            type Result = actix::ResponseActFuture<Self, ()>;

            fn handle(
                &mut self,
                _msg: $crate::scheduler::TickOrStop,
                ctx: &mut Self::Context,
            ) -> Self::Result {
                use actix::AsyncContext;
                Box::pin(
                    ctx.address()
                        .send($crate::scheduler::Tick)
                        .into_actor(self)
                        .map(|res, _, ctx| {
                            if !res.unwrap_or(false) {
                                tracing::warn!("unable to renew ts, trying to stop");
                                ctx.stop()
                            }
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
