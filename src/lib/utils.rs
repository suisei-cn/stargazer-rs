use std::time::{SystemTime, UNIX_EPOCH};

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
                    KillerActor::kill(true)
                }
            }
        }
    };
}

#[macro_export]
/// impl_task_field_getter!(Actor, scheduler)
macro_rules! impl_task_field_getter {
    ($self: ident, $scheduler: ident) => {
        impl $crate::scheduler::SchedulerGetter for $self {
            fn get_scheduler(&self) -> &actix::Addr<$crate::scheduler::actor::ScheduleActor<Self>> {
                &self.$scheduler
            }
        }
    };
}

pub fn timestamp(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}
