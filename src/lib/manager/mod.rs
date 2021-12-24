use std::marker::PhantomData;

use actix_web::web::Data;
use actix_web::{web, Scope};
use frunk_core::hlist::{HCons, HFoldLeftable, HMappable, HNil};
use frunk_core::traits::Poly;
use hmap_serde::{HLabelledMap, Labelled};
use mongodb::{Collection, Database};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tap::Pipe;

use field::FoldFieldEp;
pub use models::Vtuber;
use utils::ToOptionHList;

use crate::manager::utils::{IntoDisplay, OptionLiftF};
use crate::scheduler::Task;

mod entry;
mod errors;
mod field;
mod models;
mod ops;
mod utils;

#[derive(Debug)]
pub struct Source<T>(PhantomData<T>);

impl<T: Task> Labelled for Source<T> {
    const KEY: &'static str = T::Entry::KEY;
}

impl<T> Default for Source<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T> Copy for Source<T> {}

pub struct Manager<L> {
    sources: L,
    db: Database,
    coll: Collection<Vtuber>,
}

impl Manager<HNil> {
    pub const fn new(db: Database, coll: Collection<Vtuber>) -> Self {
        Self {
            sources: HNil,
            db,
            coll,
        }
    }
}

impl<L> Manager<L> {
    pub fn register<H: Task>(self) -> Manager<HCons<Source<H>, L>> {
        Manager {
            sources: HCons {
                head: Source::default(),
                tail: self.sources,
            },
            db: self.db,
            coll: self.coll,
        }
    }
}

impl<L> Manager<L>
where
    L: HFoldLeftable<Poly<FoldFieldEp>, Scope, Output = Scope> + ToOptionHList + Copy + 'static,
    HLabelledMap<L::OptionHList>: Serialize + DeserializeOwned,
    L::OptionHList: 'static + HMappable<Poly<OptionLiftF<IntoDisplay>>>,
    HLabelledMap<<L::OptionHList as HMappable<Poly<OptionLiftF<IntoDisplay>>>>::Output>: Serialize,
{
    #[allow(clippy::similar_names)]
    pub fn build(self, prefix: &str) -> Scope {
        let vtuber_scope = web::scope("/{vtuber}")
            .pipe(|scope| self.sources.foldl(Poly(FoldFieldEp), scope))
            .service(
                web::resource("")
                    .route(web::get().to(entry::get::<L, _, _>))
                    .route(web::delete().to(entry::delete)),
            );
        Scope::new(prefix)
            .app_data(Data::new(self.db))
            .app_data(Data::new(self.coll))
            .service(web::resource("").route(web::post().to(entry::create)))
            .service(vtuber_scope)
    }
}
