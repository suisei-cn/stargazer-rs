use actix_web::web::{Data, Json, Path};
use actix_web::HttpResponse;
use erased_serde::Serialize;
use frunk_core::hlist::HMappable;
use frunk_core::traits::Poly;
use futures::future;
use hmap_serde::HLabelledMap;
use mongodb::{Collection, Database};
use serde::de::DeserializeOwned;

use crate::db::{CollOperation, DBOperation};
use crate::manager::errors::CrudError;
use crate::manager::ops::{CreateVtuberOp, DeleteVtuberOp, GetVtuberOp};
use crate::manager::utils::{IntoDisplay, OptionLiftF, ToOptionHList};
use crate::manager::Vtuber;
use crate::utils::BoolExt;

pub async fn get<L, LO, LD>(
    name: Path<String>,
    coll: Data<Collection<Vtuber>>,
    db: Data<Database>,
) -> Result<Json<HLabelledMap<LD>>, CrudError>
where
    L: ToOptionHList<OptionHList = LO>,
    LO: HMappable<Poly<OptionLiftF<IntoDisplay>>, Output = LD>,
    HLabelledMap<LO>: DeserializeOwned,
    HLabelledMap<LD>: Serialize,
{
    let vtuber = GetVtuberOp {
        name: name.into_inner(),
    }
    .execute(&*coll.into_inner())
    .await?
    .ok_or(CrudError::MissingVtuber)?;
    let flatten = vtuber.flatten::<L::OptionHList>(&*db.into_inner()).await?;
    let wrapped = flatten.fields.0.map(Poly(OptionLiftF(IntoDisplay)));
    Ok(Json(HLabelledMap(wrapped)))
}

pub async fn delete(
    name: Path<String>,
    coll: Data<Collection<Vtuber>>,
    db: Data<Database>,
) -> Result<HttpResponse, CrudError> {
    let name = name.into_inner();
    let coll = &*coll.into_inner();
    let db = &*db.into_inner();

    let vtuber = GetVtuberOp { name: name.clone() }
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

    DeleteVtuberOp { name }
        .execute(coll)
        .await?
        .true_or(CrudError::Inconsistency)?;

    Ok(HttpResponse::NoContent().finish())
}

pub async fn create(
    name: String,
    coll: Data<Collection<Vtuber>>,
) -> Result<HttpResponse, CrudError> {
    Ok(
        if (CreateVtuberOp { name })
            .execute(&*coll.into_inner())
            .await?
        {
            HttpResponse::NoContent().finish()
        } else {
            HttpResponse::Conflict().finish()
        },
    )
}
