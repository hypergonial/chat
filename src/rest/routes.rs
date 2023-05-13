use super::rejections::handle_rejection;
use crate::gateway::handler::GATEWAY;
use crate::models::auth::Token;
use crate::models::rejections::{InternalServerError, Unauthorized};
use crate::models::rest::{CreateMessage, CreateUser, CreateUserResponse};
use crate::models::snowflake::{self, Snowflake};
use crate::models::user::User;
use crate::models::{gateway_event::GatewayEvent, message::Message};
use std::eprintln;
use std::time::Duration;
use secrecy::ExposeSecret;
use warp::filters::BoxedFilter;
use warp::http::{header, Method};
use warp::Filter;

pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    // https://javascript.info/fetch-crossorigin
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec![
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::OPTIONS,
            Method::PUT,
            Method::PATCH,
        ])
        .allow_headers(vec![
            header::CONTENT_TYPE,
            header::ORIGIN,
            header::AUTHORIZATION,
            header::CACHE_CONTROL,
        ])
        .max_age(Duration::from_secs(3600));

    let create_msg = warp::path!("message" / "create")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::header("Authorization"))
        .and(warp::body::json())
        .and_then(create_message)
        .with(cors.clone());

    let create_user = warp::path!("user" / "create")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(user_create)
        .with(cors);

    create_msg.or(create_user).recover(handle_rejection).boxed()
}

// Note: Needs to be async for the `and_then` combinator
/// Validate a token and return the parsed token data if successful.
#[inline]
async fn validate_token(token: String) -> Result<Token, warp::Rejection> {
    let Ok(token) = Token::decode(&token, "among us") else {
        return Err(warp::reject::custom(Unauthorized));
    };
    Ok(token)
}

/// Send a new message and return the message data.
///
/// ## Arguments
///
/// * `token` - The authorization token
/// * `payload` - The CreateMessage payload
/// 
/// ## Returns
/// 
/// * `impl warp::Reply` - A JSON response containing a `Message` object
///
/// ## Endpoint
///
/// POST `/message/create`
async fn create_message(
    token: String,
    payload: CreateMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let token = validate_token(token).await?;

    let user = User::fetch(token.data().user_id().into())
        .await
        .ok_or_else(|| {
            eprintln!("Failed to fetch user from database");
            warp::reject::custom(InternalServerError)
        })?;

    let message_id: Snowflake = snowflake::get_generator(1, 1).real_time_generate().into();
    let message = Message::new(message_id, user, payload.content().to_string());

    if let Err(e) = message.commit().await {
        eprintln!("Failed to commit message to database: {}", e);
        return Err(warp::reject::custom(InternalServerError));
    }

    GATEWAY.read().await.dispatch(
        message.author().id(),
        GatewayEvent::MessageCreate(message.clone()),
    );
    Ok(warp::reply::with_status(
        warp::reply::json(&message),
        warp::http::StatusCode::CREATED,
    ))
}

/// Create a new user and return the user data and token.
///
/// ## Arguments
///
/// * `payload` - The CreateUser payload, containing the username
/// 
/// ## Returns
/// 
/// * `impl warp::Reply` - A JSON response containing a `CreateUserResponse` object
///
/// ## Endpoint
///
/// POST `/user/create`
async fn user_create(payload: CreateUser) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id: Snowflake = snowflake::get_generator(1, 1).real_time_generate().into();
    let user: User = User::new(user_id, payload.username.clone());

    let Ok(token) = Token::new_for(user_id, "among us") else {
        eprintln!("Failed to create token for user: {}", user_id);
        return Err(warp::reject::custom(InternalServerError));
    };

    if let Err(e) = user.commit().await {
        eprintln!("Failed to commit user to database: {}", e);
        return Err(warp::reject::custom(InternalServerError));
    }

    let user_payload = CreateUserResponse::new(user, token.expose_secret().clone());

    Ok(warp::reply::with_status(
        warp::reply::json(&user_payload),
        warp::http::StatusCode::CREATED,
    ))
}
