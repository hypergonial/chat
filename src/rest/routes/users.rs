use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use secrecy::ExposeSecret;
use serde_json::json;

use crate::models::errors::RESTError;
use crate::models::{
    appstate::SharedState,
    auth::{Credentials, StoredCredentials, Token},
    gateway_event::{GatewayEvent, PresenceUpdatePayload},
    guild::Guild,
    requests::CreateUser,
    user::{Presence, User},
};
use crate::rest::auth::{generate_hash, validate_credentials};
use serde_json::Value;

pub fn get_router() -> Router<SharedState> {
    Router::new()
        .route("/users", post(create_user))
        .route("/users/auth", post(auth_user))
        .route("/users/@self", get(fetch_self))
        .route("/users/@self/guilds", get(fetch_self_guilds))
        .route("/users/@self/presence", patch(update_presence))
        .route("/usernames/:username", get(query_username))
}

/// Create a new user and return the user data.
///
/// ## Arguments
///
/// * `payload` - The `CreateUser` payload, containing the username and password
///
/// ## Returns
///
/// * [`User`] - A JSON response containing the created [`User`] object
///
/// ## Endpoint
///
/// POST `/users`
async fn create_user(State(app): State<SharedState>, Json(payload): Json<CreateUser>) -> Result<Json<User>, RESTError> {
    let password = payload.password.clone();

    let user = User::from_payload(&app.config, &payload)?;

    if app.db.users().fetch_user_by_username(user.username()).await.is_some() {
        return Err(RESTError::BadRequest(format!(
            "User with username {} already exists",
            user.username()
        )));
    }

    let credentials = StoredCredentials::new(user.id(), generate_hash(&password)?);

    // User needs to be created before credentials to avoid foreign key constraint
    app.db.users().create_user(&user).await?;
    credentials.commit(app).await?;

    Ok(Json(user))
}

/// Validate a user's credentials and return a token if successful.
///
/// ## Arguments
///
/// * `credentials` - The user's credentials
///
/// ## Returns
///
/// * `{"user_id": user_id, "token": token}` - A JSON response containing the session token and `user_id`
///
/// ## Endpoint
///
/// POST `/users/auth`
async fn auth_user(
    State(app): State<SharedState>,
    Json(credentials): Json<Credentials>,
) -> Result<Json<Value>, RESTError> {
    let user_id = validate_credentials(app.clone(), credentials).await?;
    let token = Token::new_for(app.config.app_secret(), user_id)?;

    Ok(Json(json!({
        "user_id": user_id,
        "token": token.expose_secret(),
    })))
}

/// Get the current user's data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
///
/// ## Returns
///
/// * [`User`] - A JSON response containing the user's data
///
/// ## Endpoint
///
/// GET `/users/@self`
async fn fetch_self(State(app): State<SharedState>, token: Token) -> Result<Json<User>, RESTError> {
    let user = app
        .db
        .users()
        .fetch_user(token.data().user_id())
        .await
        .ok_or(RESTError::NotFound("User not found".into()))?;

    Ok(Json(user))
}

/// Fetch a user's guilds.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
///
/// ## Returns
///
/// * [`Vec<Guild>`] - A JSON response containing the fetched [`Guild`] objects
///
/// ## Endpoint
///
/// GET `/users/@self/guilds`
async fn fetch_self_guilds(State(app): State<SharedState>, token: Token) -> Result<Json<Vec<Guild>>, RESTError> {
    let guilds = app.db.guilds().fetch_guilds_for_user(token.data().user_id()).await?;

    Ok(Json(guilds))
}

/// Update the token-holder's presence.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `new_presence` - The new presence to set
///
/// ## Returns
///
/// * [`Presence`] - A JSON response containing the updated [`Presence`] object
///
/// ## Errors
///
/// * [`RESTError::NotFound`] - If the user is not found
/// * [`RESTError::App`] - If the database query fails
///
/// ## Dispatches
///
/// * [`GatewayEvent::PresenceUpdate`] - For all members in guilds shared with the user
///
/// ## Endpoint
///
/// PATCH `/users/@self/presence`
pub async fn update_presence(
    State(app): State<SharedState>,
    token: Token,
    Json(new_presence): Json<Presence>,
) -> Result<Json<Presence>, RESTError> {
    let user_id_i64: i64 = token.data().user_id().into();

    sqlx::query!(
        "UPDATE users SET last_presence = $1 WHERE id = $2",
        new_presence as i16,
        user_id_i64
    )
    .execute(app.db.pool())
    .await?;

    if app.gateway.is_connected(token.data().user_id()) {
        app.gateway
            .dispatch(GatewayEvent::PresenceUpdate(PresenceUpdatePayload {
                presence: new_presence,
                user_id: token.data().user_id(),
            }));
    }

    Ok(Json(new_presence))
}

/// Check for the existence of a user with the given username.
///
/// ## Arguments
///
/// * `username` - The username to check for
///
/// ## Errors
///
/// * [`RESTError::NotFound`] - If the user is not found
/// * [`RESTError::App`] - If the database query fails
///
/// ## Endpoint
///
/// GET `/users/{username}`
pub async fn query_username(State(app): State<SharedState>, username: String) -> Result<StatusCode, RESTError> {
    sqlx::query!("SELECT id FROM users WHERE username = $1", username)
        .fetch_optional(app.db.pool())
        .await?
        .ok_or(RESTError::NotFound("User not found".into()))?;

    Ok(StatusCode::OK)
}
