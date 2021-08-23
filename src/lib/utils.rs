pub trait TypeEq {
    type Other;
}

impl<T> TypeEq for T {
    type Other = T;
}

#[macro_export]
macro_rules! impl_message_target {
    ($name: ident, $target_ty: ty) => {
        #[derive(Debug, Copy, Clone)]
        struct $name;
        impl $crate::context::MessageTarget for $name {
            type Actor = $target_ty;
            type Addr = actix::Addr<$target_ty>;
        }
    };
}

#[macro_export]
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
