use std::error::Error;
use std::ops::Deref;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use actix::Addr;
use actix_rt::task::JoinHandle;
use mongodb::error::{ErrorKind, WriteFailure};

use crate::scheduler::actor::ScheduleActor;

pub type Scheduler<T> = Addr<ScheduleActor<T>>;

pub trait TypeEq {
    type Other;
}

impl<T> TypeEq for T {
    type Other = Self;
}

#[macro_export]
macro_rules! o {
    ($opt: ident .map_or($default: expr, |$arg: ident| $f: expr)) => {
        if let Some($arg) = $opt {
            $f
        } else {
            $default
        }
    };
}

#[macro_export]
// impl_stop_on_panic!(Actor)
macro_rules! impl_stop_on_panic {
    ( $name:ident $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? ) => {
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? Drop for $name $(< $( $lt ),+ >)? {
            fn drop(&mut self) {
                if std::thread::panicking() {
                    crate::KillerActor::kill(true)
                }
            }
        }
    }
}

#[macro_export]
// impl_task_field_getter!(Actor, scheduler)
macro_rules! impl_task_field_getter {
    ($self: ident $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)?, $info: ident, $scheduler: ident) => {
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $crate::scheduler::InfoGetter for $self $(< $( $lt ),+ >)? {
            fn get_info(&self) -> $crate::scheduler::TaskInfo {
                self.$info
            }
        }
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $crate::scheduler::SchedulerGetter for $self $(< $( $lt ),+ >)? {
            fn get_scheduler(&self) -> &actix::Addr<$crate::scheduler::actor::ScheduleActor<Self>> {
                &self.$scheduler
            }
        }
    };
}

#[macro_export]
macro_rules! impl_to_collector_handler {
    ($Self: ident $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? , $entry: ident) => {
        impl<$($( $lt $( : $clt $(+ $dlt )* )? ),+ , )? Z: 'static + serde::Serialize + Send + Sync >
            actix::Handler<crate::source::ToCollector<Z>> for $Self $(< $( $lt ),+ >)?
        {
            type Result = actix::ResponseActFuture<Self, ()>;

            fn handle(
                &mut self,
                msg: crate::source::ToCollector<Z>,
                _ctx: &mut Self::Context,
            ) -> Self::Result {
                use actix::ActorFutureExt;
                use actix::ActorContext;
                use actix::WrapFuture;
                use tracing_actix::ActorInstrument;

                use crate::request::RequestTrait;
                use crate::scheduler::InfoGetter;
                use crate::scheduler::SchedulerGetter;

                let root = self.$entry.root.clone();
                Box::pin(
                    self.get_scheduler()
                        .send(crate::scheduler::messages::CheckOwnership{ info: self.get_info() })
                        .into_actor(self)
                        .map(move |res, _act, ctx| {
                            let holding_ownership = res.unwrap_or(Ok(false)).unwrap_or(false);
                            if holding_ownership {
                                crate::context::ArbiterContext::with(|ctx| {
                                    ctx.send::<crate::collector::CollectorActor, _>(
                                        crate::collector::Publish::new(root, &*msg.topic, msg.body),
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

#[allow(clippy::cast_possible_truncation, clippy::missing_panics_doc)]
pub fn timestamp(t: SystemTime) -> i64 {
    // SAFETY: t >= 0
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

pub trait BoolExt {
    #[allow(clippy::missing_errors_doc)]
    fn true_or<E>(self, e: E) -> Result<(), E>;
}

impl BoolExt for bool {
    fn true_or<E>(self, e: E) -> Result<(), E> {
        self.then(|| ()).ok_or(e)
    }
}

pub trait FromStrE: Sized {
    type Err: Error;
    #[allow(clippy::missing_errors_doc)]
    fn from_str_e(s: &str) -> Result<Self, Self::Err>;
}

impl<T, E> FromStrE for T
where
    T: FromStr<Err = E>,
    E: Error,
{
    type Err = E;

    fn from_str_e(s: &str) -> Result<Self, Self::Err> {
        FromStr::from_str(s)
    }
}

pub trait DBErrorExt {
    fn write(&self) -> Option<i32>;
}

impl DBErrorExt for mongodb::error::Error {
    fn write(&self) -> Option<i32> {
        match &*self.kind {
            ErrorKind::Write(WriteFailure::WriteError(e)) => Some(e.code),
            _ => None,
        }
    }
}
