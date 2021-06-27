// TODO: unfinished macro that implements handler for Dispatcher
#[macro_export]
macro_rules! impl_handler {
    ($type:ty, $blk: block) => {
        impl_handler!($type, Result<()>, $blk);
    };
    ($type:ty, $rt_type:ty, $blk: block) => {
        impl Handler<$type> for Dispatcher {
            type Result = $rt_type;

            fn handle(&mut self, msg: Event<String>, ctx: &mut Context<Self>) -> Self::Result
                $blk
        }
    };
}
