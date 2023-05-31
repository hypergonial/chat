use secrecy::ExposeSecret;
use serde_json::json;
use warp::{filters::BoxedFilter, Filter};

use super::common::needs_token;
use crate::dispatch;
use crate::models::{
    appstate::APP,
    auth::{Credentials, StoredCredentials, Token},
    gateway_event::{GatewayEvent, PresenceUpdatePayload},
    guild::Guild,
    rejections::{BadRequest, InternalServerError, NotFound, Unauthorized},
    rest::CreateUser,
    user::{Presence, User},
};
use crate::rest::auth::{generate_hash, validate_credentials};
use crate::utils::traits::{OptionExt, ResultExt};

/// Get all routes under `/users`.
pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    let create_user = warp::path!("users")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_user);

    let login = warp::path!("users" / "auth")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(auth_user);

    let query_self = warp::path!("users" / "@self")
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_self);

    let fetch_self_guilds = warp::path!("users" / "@self" / "guilds")
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_self_guilds);

    let update_presence = warp::path!("users" / "@self" / "presence")
        .and(warp::patch())
        .and(needs_token())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(update_presence);

    let query_username = warp::path!("usernames" / String)
        .and(warp::get())
        .and_then(query_username);

    create_user
        .or(login)
        .or(query_self)
        .or(fetch_self_guilds)
        .or(update_presence)
        .or(query_username)
        .boxed()
}

/// Create a new user and return the user data and token.
///
/// ## Arguments
///
/// * `payload` - The CreateUser payload, containing the username and password
///
/// ## Returns
///
/// * [`User`] - A JSON response containing the created [`User`] object
///
/// ## Endpoint
///
/// POST `/users`
async fn create_user(payload: CreateUser) -> Result<impl warp::Reply, warp::Rejection> {
    let password = payload.password.clone();

    let user = match User::from_payload(payload).await {
        Ok(user) => user,
        Err(e) => {
            tracing::debug!("Invalid user payload: {}", e);
            return Err(warp::reject::custom(BadRequest::new(e.to_string().as_ref())));
        }
    };

    if User::fetch_by_username(user.username()).await.is_some() {
        tracing::debug!("User with username {} already exists", user.username());
        return Err(warp::reject::custom(BadRequest::new(
            format!("User with username {} already exists", user.username()).as_ref(),
        )));
    }

    let credentials = StoredCredentials::new(
        user.id(),
        generate_hash(&password).or_reject_and_log(
            InternalServerError::new("Credential generation failed"),
            "Failed to generate password hash",
        )?,
    );

    // User needs to be committed before credentials to avoid foreign key constraint
    if let Err(e) = user.commit().await {
        tracing::error!("Failed to commit user to database: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    } else if let Err(e) = credentials.commit().await {
        tracing::error!("Failed to commit credentials to database: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&user),
        warp::http::StatusCode::CREATED,
    ))
}

/// Validate a user's credentials and return a token if successful.
///
/// ## Arguments
///
/// * `credentials` - The user's credentials
///
/// ## Returns
///
/// * `{"user_id": user_id, "token": token}` - A JSON response containing the session token and user_id
///
/// ## Endpoint
///
/// POST `/users/auth`
async fn auth_user(credentials: Credentials) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id = validate_credentials(credentials)
        .await
        .or_reject(Unauthorized::new("Invalid credentials"))?;

    let token = Token::new_for(user_id, "among us").or_reject_and_log(
        InternalServerError::new("Failed to generate token"),
        format!("Failed to generate token for user {}", user_id).as_ref(),
    )?;

    Ok(warp::reply::with_status(
        warp::reply::json(&json!({"user_id": user_id, "token": token.expose_secret()})),
        warp::http::StatusCode::OK,
    ))
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
async fn fetch_self(token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let user = User::fetch(token.data().user_id())
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to fetch user from database")?;

    Ok(warp::reply::with_status(
        warp::reply::json(&user),
        warp::http::StatusCode::OK,
    ))
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
async fn fetch_self_guilds(token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let guilds = Guild::fetch_all_for_user(token.data().user_id()).await.map_err(|e| {
        tracing::error!(message = "Failed to fetch user guilds from database", user = %token.data().user_id(), error = %e);
        warp::reject::custom(InternalServerError::db())
    })?;

    Ok(warp::reply::with_status(
        warp::reply::json(&guilds),
        warp::http::StatusCode::OK,
    ))
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
/// ## Dispatches
///
/// * [`GatewayEvent::PresenceUpdate`] - For all members in guilds shared with the user
///
/// ## Endpoint
///
/// PATCH `/users/@self/presence`
pub async fn update_presence(token: Token, new_presence: Presence) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id_i64: i64 = token.data().user_id().into();
    let db = &APP.read().await.db;

    sqlx::query!(
        "UPDATE users SET last_presence = $1 WHERE id = $2",
        new_presence as i16,
        user_id_i64
    )
    .execute(db.pool())
    .await
    .map_err(|e| {
        tracing::error!(message = "Failed to update user presence", user = %token.data().user_id(), error = %e);
        warp::reject::custom(InternalServerError::db())
    })?;

    if APP.read().await.gateway.is_connected(token.data().user_id()) {
        dispatch!(GatewayEvent::PresenceUpdate(PresenceUpdatePayload {
            presence: new_presence,
            user_id: token.data().user_id(),
        }));
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&new_presence),
        warp::http::StatusCode::OK,
    ))
}

/// Check for the existence of a user with the given username.
///
/// ## Arguments
///
/// * `username` - The username to check for
///
/// ## Endpoint
///
/// GET `/users/{username}`
pub async fn query_username(username: String) -> Result<impl warp::Reply, warp::Rejection> {
    let db = &APP.read().await.db;

    sqlx::query!("SELECT id FROM users WHERE username = $1", username)
        .fetch_optional(db.pool())
        .await
        .ok()
        .or_reject(NotFound::new("User not found"))?;

    Ok(warp::reply::reply())
}
