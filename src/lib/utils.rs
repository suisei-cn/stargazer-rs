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
/// impl_task_field_getter!(Actor, info, collection, scheduler)
macro_rules! impl_task_field_getter {
    ($self: ident, $info: ident, $collection: ident, $scheduler: ident) => {
        impl $crate::scheduler::TaskInfoGetter for $self {
            fn get_info(&self) -> TaskInfo {
                self.$info
            }
            fn get_info_mut(&mut self) -> &mut $crate::scheduler::models::TaskInfo {
                &mut self.$info
            }
        }

        impl $crate::scheduler::CollectionGetter for $self {
            fn get_collection(&self) -> &$crate::db::Collection {
                &self.$collection
            }
        }

        impl $crate::scheduler::SchedulerGetter for $self {
            fn get_scheduler(&self) -> &actix::Addr<$crate::scheduler::actor::ScheduleActor<Self>> {
                &self.$scheduler
            }
        }
    };
}
