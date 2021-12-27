use actix::{Actor, Context, Handler, Message, System};
use uuid::Uuid;

mod utils {
    use std::convert::Infallible;
    use std::fmt::{Debug, Display, Formatter};
    use std::marker::PhantomData;
    use std::rc::Rc;
    use std::str::FromStr;

    use actix::{Actor, Context};
    use actix_signal::SignalHandler;
    use hmap_serde::Labelled;
    use serde::{Deserialize, Serialize};
    use tracing::Span;

    use crate::db::Document;
    use crate::scheduler::{Entry, Task, TaskInfo};
    use crate::utils::Scheduler;

    pub trait StaticUnpinned: 'static + Unpin {}

    #[derive(SignalHandler)]
    pub struct A<T: StaticUnpinned, U: StaticUnpinned + Copy + Eq> {
        _marker_1: PhantomData<(T, U)>,
        _marker_2: Rc<()>,
        info: TaskInfo,
        entry: Entry<()>,
        scheduler: Scheduler<Self>,
    }

    impl<T: StaticUnpinned, U: StaticUnpinned + Copy + Eq> Debug for A<T, U> {
        fn fmt(&self, _: &mut Formatter<'_>) -> std::fmt::Result {
            unimplemented!()
        }
    }

    impl<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned> Actor for A<T, U> {
        type Context = Context<Self>;
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct AEntry;

    impl Labelled for AEntry {
        const KEY: &'static str = "test_task";
    }

    impl FromStr for AEntry {
        type Err = Infallible;

        fn from_str(_: &str) -> Result<Self, Self::Err> {
            Ok(Self)
        }
    }

    impl Display for AEntry {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "")
        }
    }

    impl<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned> Task for A<T, U> {
        type Entry = AEntry;
        type Ctor = ();

        fn query() -> Document {
            unimplemented!()
        }

        fn construct(
            _: Entry<Self::Entry>,
            _: Self::Ctor,
            _: Scheduler<Self>,
            _: TaskInfo,
        ) -> Self {
            unimplemented!()
        }

        fn span(&self) -> Span {
            unimplemented!()
        }
    }

    impl_stop_on_panic!(A<T: StaticUnpinned, U: StaticUnpinned+ Copy + Eq>);
    impl_to_collector_handler!(A<T: StaticUnpinned, U: Copy + Eq + StaticUnpinned>, entry);
    impl_task_field_getter!(A<T: StaticUnpinned, U: StaticUnpinned+ Copy + Eq>, info, scheduler);
}
