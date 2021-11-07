use actix_web::web::{Data, Json, Path};
use actix_web::{web, Scope};
use frunk_core::traits::Func;
use mongodb::{Collection, Database};

use crate::db::{CollOperation, DBOperation};
use crate::scheduler::Task;

use super::errors::CrudError;
use super::models::Vtuber;
use super::ops::GetVtuberOp;
use super::Source;

pub struct FoldFieldGetter;

async fn _field_getter<T: Task>(
    name: String,
    coll: &Collection<Vtuber>,
    db: &Database,
) -> Result<T::Entry, CrudError> {
    let vtuber = GetVtuberOp::new(name)
        .execute(coll)
        .await?
        .ok_or(CrudError::MissingVtuber)?;
    let db_ref = vtuber
        .fields
        .get(T::NAMESPACE)
        .ok_or(CrudError::MissingField(T::NAMESPACE))?;
    Ok(db_ref
        .get()
        .execute(db)
        .await?
        .ok_or(CrudError::Inconsistency)?)
}

impl<T: Task> Func<(Scope, Source<T>)> for FoldFieldGetter {
    type Output = Scope;

    fn call((acc, _): (Scope, Source<T>)) -> Self::Output {
        let field_getter =
            |name: Path<String>, coll: Data<Collection<Vtuber>>, db: Data<Database>| async {
                _field_getter::<T>(name.into_inner(), &*coll.into_inner(), &*db.into_inner())
                    .await
                    .map(Json)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + 'static>)
            };
        acc.service(web::resource(format!("/{}", T::NAMESPACE)).route(web::get().to(field_getter)))
    }
}
