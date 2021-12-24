use actix_web::web::{Data, Path};
use actix_web::{web, HttpResponse, Scope};
use frunk_core::traits::Func;
use hmap_serde::Labelled;
use mongodb::{Collection, Database};

use crate::db::{CollOperation, DBOperation};
use crate::manager::ops::{CreateFieldOp, LinkRefOp};
use crate::scheduler::Task;
use crate::utils::{BoolExt, FromStrE};

use super::errors::CrudError;
use super::models::Vtuber;
use super::ops::GetVtuberOp;
use super::Source;

pub struct FoldFieldEp;

async fn get<T: Task>(
    name: Path<String>,
    coll: Data<Collection<Vtuber>>,
    db: Data<Database>,
) -> Result<String, CrudError> {
    let vtuber = GetVtuberOp::new(name.into_inner())
        .execute(&*coll.into_inner())
        .await?
        .ok_or(CrudError::MissingVtuber)?;
    let db_ref = vtuber
        .fields
        .get(T::Entry::KEY)
        .ok_or(CrudError::MissingField)?;
    Ok(db_ref
        .get::<T::Entry>()
        .execute(&*db.into_inner())
        .await?
        .ok_or(CrudError::Inconsistency)?
        .to_string())
}

async fn put<T: Task>(
    name: Path<String>,
    coll: Data<Collection<Vtuber>>,
    db: Data<Database>,
    payload: String,
) -> Result<HttpResponse, CrudError> {
    let name = name.into_inner();
    let coll = &*coll.into_inner();
    let db = &*db.into_inner();
    let payload = T::Entry::from_str_e(payload.as_str()).map_err(|e| CrudError::InvalidValue {
        value: payload,
        source: Box::new(e),
    })?;

    let vtuber = GetVtuberOp::new(name.clone())
        .execute(coll)
        .await?
        .ok_or(CrudError::MissingVtuber)?;
    let db_ref = vtuber.fields.get(T::Entry::KEY);
    if let Some(db_ref) = db_ref {
        db_ref
            .set(payload)
            .execute(db)
            .await?
            .map(|_| HttpResponse::NoContent().finish())
            .ok_or(CrudError::Inconsistency)
    } else {
        let db_ref = CreateFieldOp::new(payload)
            .execute(&db.collection::<T::Entry>(T::Entry::KEY))
            .await?;
        LinkRefOp::new(name, T::Entry::KEY, Some(db_ref))
            .execute(coll)
            .await?;
        Ok(HttpResponse::NoContent().finish())
    }
}

async fn delete<T: Task>(
    name: Path<String>,
    coll: Data<Collection<Vtuber>>,
    db: Data<Database>,
) -> Result<HttpResponse, CrudError> {
    let name = name.into_inner();
    let coll = &*coll.into_inner();
    let db = &*db.into_inner();

    let vtuber = GetVtuberOp::new(name.clone())
        .execute(coll)
        .await?
        .ok_or(CrudError::MissingVtuber)?;
    let db_ref = vtuber
        .fields
        .get(T::Entry::KEY)
        .ok_or(CrudError::MissingField)?;

    // Delete referenced document.
    db_ref
        .del()
        .execute(db)
        .await?
        .true_or(CrudError::Inconsistency)?;
    // Delete reference field.
    LinkRefOp::new(name, T::Entry::KEY, None)
        .execute(coll)
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

impl<T: Task> Func<(Scope, Source<T>)> for FoldFieldEp {
    type Output = Scope;

    fn call((acc, _): (Scope, Source<T>)) -> Self::Output {
        acc.service(
            web::resource(format!("/{}", T::Entry::KEY))
                .route(web::get().to(get::<T>))
                .route(web::put().to(put::<T>))
                .route(web::delete().to(delete::<T>)),
        )
    }
}
