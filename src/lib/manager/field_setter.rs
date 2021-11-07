use actix_web::http::StatusCode;
use actix_web::web::{Data, Json, Path};
use actix_web::{web, HttpResponse, Scope};
use frunk_core::traits::Func;
use mongodb::{Collection, Database};

use crate::db::{CollOperation, DBOperation};
use crate::manager::ops::{CreateFieldOp, LinkRefOp};
use crate::scheduler::Task;

use super::errors::CrudError;
use super::models::Vtuber;
use super::ops::GetVtuberOp;
use super::Source;

pub struct FoldFieldSetter;

async fn _field_setter<T: Task>(
    name: String,
    coll: &Collection<Vtuber>,
    db: &Database,
    payload: T::Entry,
) -> Result<(), CrudError> {
    let vtuber = GetVtuberOp::new(name.clone())
        .execute(coll)
        .await?
        .ok_or(CrudError::MissingVtuber)?;
    let db_ref = vtuber.fields.get(T::NAMESPACE);
    if let Some(db_ref) = db_ref {
        db_ref
            .set(payload)
            .execute(db)
            .await?
            .map(|_| ())
            .ok_or(CrudError::Inconsistency)
    } else {
        let db_ref = CreateFieldOp::new(payload)
            .execute(&db.collection::<T::Entry>(T::NAMESPACE))
            .await?;
        Ok(LinkRefOp::new(name, T::NAMESPACE, db_ref)
            .execute(coll)
            .await?)
    }
}

impl<T: Task> Func<(Scope, Source<T>)> for FoldFieldSetter {
    type Output = Scope;

    fn call((acc, _): (Scope, Source<T>)) -> Self::Output {
        let field_setter = |name: Path<String>,
                            coll: Data<Collection<Vtuber>>,
                            db: Data<Database>,
                            payload: Json<T::Entry>| async {
            _field_setter::<T>(
                name.into_inner(),
                &*coll.into_inner(),
                &*db.into_inner(),
                payload.into_inner(),
            )
            .await
            .map(|_| HttpResponse::new(StatusCode::NO_CONTENT))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + 'static>)
        };
        acc.service(web::resource(format!("/{}", T::NAMESPACE)).route(web::put().to(field_setter)))
    }
}
