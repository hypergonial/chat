use std::time::Duration;

use axum::Router;
use hyper::{header, Method};
use tower_http::cors::{Any, CorsLayer};

use super::channels::get_router as get_channel_router;
use super::guilds::get_router as get_guild_router;
use super::users::get_router as get_user_router;

/// Get all routes for the REST API. Includes CORS.
pub fn get_router() -> Router {
    // https://javascript.info/fetch-crossorigin
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::OPTIONS,
            Method::PUT,
            Method::PATCH,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ORIGIN,
            header::AUTHORIZATION,
            header::CACHE_CONTROL,
        ])
        .max_age(Duration::from_secs(3600));

    Router::new()
        .nest("/channels", get_channel_router())
        .nest("/guilds", get_guild_router())
        .nest("/users", get_user_router())
        .layer(cors)
}

/* pub type SharedIDLimiter = Arc<RateLimiter<u64, DashMapStateStore<u64>, QuantaClock, NoOpMiddleware<QuantaInstant>>>; */

/* /// A filter that checks for and validates a token.
pub fn needs_token() -> impl Filter<Extract = (Token,), Error = warp::Rejection> + Clone {
    warp::header("authorization").and_then(validate_token)
}

/// A filter that checks for and validates a token, and enforces a rate limit.
pub fn needs_limit(id_limiter: SharedIDLimiter) -> impl Filter<Extract = (Token,), Error = warp::Rejection> + Clone {
    needs_token()
        .and(warp::any())
        .map(move |t| (t, id_limiter.clone()))
        .untuple_one()
        .and_then(validate_limit)
} */

/* // Note: Needs to be async for the `and_then` combinator
/// Validate a token and return the parsed token data if successful.
#[inline]
async fn validate_token(token: String) -> Result<Token, warp::Rejection> {
    Token::validate(&token, "among us")
        .await
        .or_reject(Unauthorized::bearer("app"))
}

/// Check the limiter with the key being the token's user_id
#[inline]
async fn validate_limit(token: Token, limiter: SharedIDLimiter) -> Result<Token, warp::Rejection> {
    let user_id = token.data().user_id();
    limiter.check_key(&user_id.into()).map_err(|e| {
        warp::reject::custom(BadRequest::new(
            format!("Rate limit exceeded, try again at: {:?}", e.earliest_possible()).as_ref(),
        ))
    })?;
    Ok(token)
}
 */
