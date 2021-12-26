use std::marker::PhantomData;
use std::sync::Arc;

use actix::Addr;
use hmap_serde::Labelled;
use mongodb::{Collection, Database};

use crate::db::Document;
use crate::scheduler::driver::ScheduleDriverActor;
use crate::scheduler::{ScheduleActor, Task};
use crate::ScheduleConfig;

#[derive(Clone)]
pub struct ScheduleActorBuilder<T, COLL, CTOR, CONF, DRV>
where
    T: Task,
{
    collection: Option<Collection<Document>>,
    ctor_builder: Option<Arc<dyn Fn() -> T::Ctor + Send + Sync>>,
    config: Option<ScheduleConfig>,
    driver: Option<Addr<ScheduleDriverActor<T>>>,
    _marker: PhantomData<(COLL, CTOR, CONF, DRV)>,
}

pub struct BN;

pub struct BF;

impl<T: Task> Default for ScheduleActorBuilder<T, BN, BN, BN, BN> {
    fn default() -> Self {
        Self {
            collection: None,
            ctor_builder: None,
            config: None,
            driver: None,
            _marker: PhantomData,
        }
    }
}

impl<T, COLL, CTOR, CONF, DRV> ScheduleActorBuilder<T, COLL, CTOR, CONF, DRV>
where
    T: Task,
{
    pub fn db(self, db: &Database) -> ScheduleActorBuilder<T, BF, CTOR, CONF, DRV> {
        ScheduleActorBuilder {
            collection: Some(db.collection(T::Entry::KEY)),
            ctor_builder: self.ctor_builder,
            config: self.config,
            driver: self.driver,
            _marker: PhantomData,
        }
    }
    pub fn ctor_builder(
        self,
        f: impl Fn() -> T::Ctor + Send + Sync + 'static,
    ) -> ScheduleActorBuilder<T, COLL, BF, CONF, DRV> {
        ScheduleActorBuilder {
            collection: self.collection,
            ctor_builder: Some(Arc::new(f) as Arc<dyn Fn() -> T::Ctor + Send + Sync>),
            config: self.config,
            driver: self.driver,
            _marker: PhantomData,
        }
    }
    pub fn config(self, config: ScheduleConfig) -> ScheduleActorBuilder<T, COLL, CTOR, BF, DRV> {
        ScheduleActorBuilder {
            collection: self.collection,
            ctor_builder: self.ctor_builder,
            config: Some(config),
            driver: self.driver,
            _marker: PhantomData,
        }
    }
    pub fn driver(
        self,
        driver: Addr<ScheduleDriverActor<T>>,
    ) -> ScheduleActorBuilder<T, COLL, CTOR, CONF, BF> {
        ScheduleActorBuilder {
            collection: self.collection,
            ctor_builder: self.ctor_builder,
            config: self.config,
            driver: Some(driver),
            _marker: PhantomData,
        }
    }
}

impl<T> ScheduleActorBuilder<T, BF, BF, BF, BF>
where
    T: Task,
{
    pub fn build(self) -> ScheduleActor<T> {
        // SAFETY ensured by type parameters
        ScheduleActor {
            collection: self.collection.unwrap(),
            ctor_builder: self.ctor_builder.unwrap(),
            config: self.config.unwrap(),
            ctx: Default::default(),
            driver: self.driver.unwrap(),
        }
    }
}
