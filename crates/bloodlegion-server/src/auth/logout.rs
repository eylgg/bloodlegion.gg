use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;

use crate::extract::SameOrigin;
use crate::{Result, State, auth::session::end_session};

pub async fn handler(
    State { pool, .. }: State,
    _: SameOrigin,
    jar: CookieJar,
) -> Result<impl IntoResponse> {
    Ok(end_session(&pool, jar).await)
}
