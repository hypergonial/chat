use super::auth::{generate_hash, validate_credentials};
use super::rejections::handle_rejection;
use crate::dispatch;
use crate::models::appstate::APP;
use crate::models::auth::{Credentials, StoredCredentials, Token};
use crate::models::rejections::{BadRequest, InternalServerError, Unauthorized};
use crate::models::rest::{CreateMessage, CreateUser};
use crate::models::snowflake::Snowflake;
use crate::models::user::User;
use crate::models::{gateway_event::GatewayEvent, message::Message};
use governor::clock::{QuantaClock, QuantaInstant};
use governor::middleware::NoOpMiddleware;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};
use nonzero_ext::nonzero;
use secrecy::ExposeSecret;
use std::sync::Arc;
use std::time::Duration;
use warp::filters::BoxedFilter;
use warp::http::{header, Method};
use warp::Filter;

type SharedIDLimiter =
    Arc<RateLimiter<u64, DashMapStateStore<u64>, QuantaClock, NoOpMiddleware<QuantaInstant>>>;

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

    let message_create_lim: SharedIDLimiter = Arc::new(RateLimiter::keyed(
        Quota::per_second(nonzero!(5u32)).allow_burst(nonzero!(5u32)),
    ));

    let create_msg = warp::path!("message" / "create")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::header("authorization"))
        .and_then(validate_token)
        .and(with_id_limiter(message_create_lim))
        .and_then(validate_limit)
        .and(warp::body::json())
        .and_then(create_message)
        .with(cors.clone());

    let create_user = warp::path!("user" / "create")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(user_create)
        .with(cors.clone());

    let login = warp::path!("user" / "auth")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(user_auth)
        .with(cors);

    create_msg
        .or(create_user)
        .or(login)
        .recover(handle_rejection)
        .boxed()
}

// Note: Needs to be async for the `and_then` combinator
/// Validate a token and return the parsed token data if successful.
async fn validate_token(token: String) -> Result<Token, warp::Rejection> {
    Token::decode(&token, "among us").map_err(|_| {
        warp::reject::custom(Unauthorized {
            message: "Invalid or expired token".into(),
        })
    })
}

// Check the limiter with the key being the token's user_id
async fn validate_limit(token: Token, limiter: SharedIDLimiter) -> Result<Token, warp::Rejection> {
    let user_id = token.data().user_id();
    limiter.check_key(&user_id).map_err(|e| {
        warp::reject::custom(BadRequest {
            message: format!(
                "Rate limit exceeded, try again at: {:?}",
                e.earliest_possible()
            ),
        })
    })?;
    Ok(token)
}

pub fn with_id_limiter(
    limiter: SharedIDLimiter,
) -> impl Filter<Extract = (SharedIDLimiter,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || limiter.clone())
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
    token: Token,
    payload: CreateMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let user = User::fetch(token.data().user_id().into())
        .await
        .ok_or_else(|| {
            tracing::error!("Failed to fetch user from database");
            warp::reject::custom(InternalServerError {
                message: "A database transaction error occured.".into(),
            })
        })?;

    let message = Message::new(
        Snowflake::gen_new().await,
        user,
        payload.content().to_string(),
        payload.nonce().clone(),
    );

    if let Err(e) = message.commit().await {
        tracing::error!("Failed to commit message to database: {}", e);
        return Err(warp::reject::custom(InternalServerError {
            message: "A database transaction error occured.".into(),
        }));
    }

    dispatch!(GatewayEvent::MessageCreate(message.clone()));
    Ok(warp::reply::with_status(
        warp::reply::json(&message),
        warp::http::StatusCode::CREATED,
    ))
}

/// Create a new user and return the user data and token.
///
/// ## Arguments
///
/// * `payload` - The CreateUser payload, containing the username and password
///
/// ## Returns
///
/// * `impl warp::Reply` - A JSON response containing the created `User` object
///
/// ## Endpoint
///
/// POST `/user/create`
async fn user_create(payload: CreateUser) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id: Snowflake = Snowflake::gen_new().await;

    let user = match User::new(user_id, payload.username.clone()) {
        Ok(user) => user,
        Err(e) => {
            tracing::debug!("Invalid user payload: {}", e);
            return Err(warp::reject::custom(BadRequest {
                message: e.to_string(),
            }));
        }
    };

    if User::fetch_by_username(&payload.username).await.is_some() {
        tracing::debug!("User with username {} already exists", payload.username);
        return Err(warp::reject::custom(BadRequest {
            message: format!("User with username {} already exists", payload.username),
        }));
    }

    let credentials = StoredCredentials::new(
        user_id.into(),
        generate_hash(&payload.password).expect("Failed to generate password hash"),
    );

    // User needs to be committed before credentials to avoid foreign key constraint
    if let Err(e) = user.commit().await {
        tracing::error!("Failed to commit user to database: {}", e);
        return Err(warp::reject::custom(InternalServerError {
            message: "A database transaction error occured.".into(),
        }));
    } else if let Err(e) = credentials.commit().await {
        tracing::error!("Failed to commit credentials to database: {}", e);
        return Err(warp::reject::custom(InternalServerError {
            message: "A database transaction error occured.".into(),
        }));
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
/// * `impl warp::Reply` - A JSON response containing the session token
///
/// ## Endpoint
///
/// GET `/user/auth`
async fn user_auth(credentials: Credentials) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id = match validate_credentials(credentials).await {
        Ok(user_id) => user_id,
        Err(e) => {
            tracing::debug!("Failed to validate credentials: {}", e);
            return Err(warp::reject::custom(Unauthorized {
                message: "Invalid credentials".into(),
            }));
        }
    };

    let Ok(token) = Token::new_for(user_id.into(), "among us") else {
        tracing::error!("Failed to create token for user: {}", user_id);
        return Err(warp::reject::custom(InternalServerError { message: "Failed to generate session token.".into() }));
    };

    // Return the cookie in a json response
    Ok(warp::reply::with_status(
        warp::reply::json(&token.expose_secret()),
        warp::http::StatusCode::OK,
    ))
}
