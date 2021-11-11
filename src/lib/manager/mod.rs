use std::marker::PhantomData;

use actix_web::web::Data;
use actix_web::Scope;
use frunk_core::hlist::{HCons, HFoldLeftable, HNil};
use frunk_core::traits::Poly;
use mongodb::{Collection, Database};

use field_getter::FoldFieldGetter;
use field_setter::FoldFieldSetter;
pub use models::Vtuber;

use crate::scheduler::Task;

mod errors;
mod field_getter;
mod field_setter;
mod models;
mod ops;
mod utils;

#[derive(Debug)]
pub struct Source<T>(PhantomData<T>);

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
    L: HFoldLeftable<Poly<FoldFieldGetter>, Scope, Output = Scope>
        + HFoldLeftable<Poly<FoldFieldSetter>, Scope, Output = Scope>
        + Copy,
{
    #[allow(clippy::similar_names)]
    pub fn build(self, prefix: &str) -> Scope {
        let field_getter_scope = self
            .sources
            .foldl(Poly(FoldFieldGetter), Scope::new("/{vtuber}"));
        let field_setter_scope = self
            .sources
            .foldl(Poly(FoldFieldSetter), Scope::new("/{vtuber}"));
        Scope::new(prefix)
            .app_data(Data::new(self.db))
            .app_data(Data::new(self.coll))
            .service(field_getter_scope)
            .service(field_setter_scope)
    }
}
