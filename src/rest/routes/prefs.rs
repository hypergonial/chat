use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};

use crate::models::{auth::Token, prefs::Prefs};
use crate::models::{errors::RESTError, requests::UpdatePrefs, state::App};

pub fn get_router() -> Router<App> {
    Router::new()
        .route("/prefs", get(get_prefs))
        .route("/prefs", patch(update_prefs))
}

async fn get_prefs(State(app): State<App>, token: Token) -> Result<Json<Prefs>, RESTError> {
    Prefs::fetch(app, token.data().user_id())
        .await
        .map(Json)
        .map_err(Into::into)
}

async fn update_prefs(
    State(app): State<App>,
    token: Token,
    Json(payload): Json<UpdatePrefs>,
) -> Result<StatusCode, RESTError> {
    let mut prefs = Prefs::fetch(app.clone(), token.data().user_id()).await?;
    prefs.update(payload);
    prefs.commit(app).await?;
    Ok(StatusCode::NO_CONTENT)
}
