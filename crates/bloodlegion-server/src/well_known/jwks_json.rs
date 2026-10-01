use axum::Json;

use crate::auth::jwk::{WebKeySet, list_jwks};
use crate::{Result, State};

pub async fn handler(State { pool, .. }: State) -> Result<Json<WebKeySet>> {
    Ok(Json(list_jwks(&pool).await?))
}
