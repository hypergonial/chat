use axum::{
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};

use crate::models::errors::RESTError;
use crate::models::{
    auth::Token,
    prefs::{Prefs, PrefsUpdate},
};

pub fn get_router() -> Router {
    Router::new()
        .route("/prefs", get(get_prefs))
        .route("/prefs", patch(update_prefs))
}

async fn get_prefs(token: Token) -> Result<Json<Prefs>, RESTError> {
    Prefs::fetch(token.data().user_id()).await.map(Json).map_err(Into::into)
}

async fn update_prefs(token: Token, Json(payload): Json<PrefsUpdate>) -> Result<StatusCode, RESTError> {
    let mut prefs = Prefs::fetch(token.data().user_id()).await?;
    prefs.update(payload);
    prefs.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
