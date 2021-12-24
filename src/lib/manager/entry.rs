use actix_web::HttpResponse;
use actix_web::web::{Data, Json, Path};
use futures::future;
use hmap_serde::HLabelledMap;
use mongodb::{Collection, Database};
use serde::de::DeserializeOwned;

use crate::db::{CollOperation, DBOperation};
use crate::manager::errors::CrudError;
use crate::manager::models::VtuberFlatten;
use crate::manager::ops::{CreateVtuberOp, DeleteVtuberOp, GetVtuberOp};
use crate::manager::utils::ToOptionHList;
use crate::manager::Vtuber;
use crate::utils::BoolExt;

pub async fn get<L>(
    name: Path<String>,
    coll: Data<Collection<Vtuber>>,
    db: Data<Database>,
) -> Result<Json<VtuberFlatten<L::OptionHList>>, CrudError>
    where
        L: ToOptionHList,
        HLabelledMap<L::OptionHList>: DeserializeOwned,
{
    let vtuber = GetVtuberOp::new(name.into_inner())
        .execute(&*coll.into_inner())
        .await?
        .ok_or(CrudError::MissingVtuber)?;
    let flatten = vtuber.unfold(&*db.into_inner()).await?;
    Ok(Json(flatten))
}

pub async fn delete(
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

    future::join_all(
        vtuber
            .fields
            .values()
            .map(|db_ref| db_ref.del().execute(db)),
    )
        .await
        .into_iter()
        // Ok(false) and Err(_) are considered failure
        .find(|res| !*res.as_ref().unwrap_or(&false))
        .unwrap_or(Ok(true))
        // Err(_) are db errors.
        .map_err(CrudError::DBError)
        // Ok(false) are caused by missing refs.
        .and_then(|e| e.true_or(CrudError::Inconsistency))?;

    DeleteVtuberOp::new(name)
        .execute(coll)
        .await?
        .true_or(CrudError::Inconsistency)?;

    Ok(HttpResponse::NoContent().finish())
}

pub async fn create(name: String, coll: Data<Collection<Vtuber>>) -> Result<HttpResponse, CrudError> {
    CreateVtuberOp::new(name)
        .execute(&*coll.into_inner())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}
