use std::{sync::Arc, time::Duration};

use governor::{
    clock::{QuantaClock, QuantaInstant},
    middleware::NoOpMiddleware,
    state::keyed::DashMapStateStore,
    RateLimiter,
};
use warp::{
    filters::BoxedFilter,
    http::{header, Method},
    Filter,
};

use super::channels::get_routes as get_channel_routes;
use super::guilds::get_routes as get_guild_routes;
use super::users::get_routes as get_user_routes;
use crate::rest::rejections::handle_rejection;

use crate::models::{
    auth::Token,
    rejections::{BadRequest, Unauthorized},
};
use crate::utils::traits::ResultExt;

/// Get all routes for the REST API. These routes include error handling and CORS.
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

    get_channel_routes()
        .or(get_guild_routes())
        .or(get_user_routes())
        .recover(handle_rejection)
        .with(cors)
        .boxed()
}

pub type SharedIDLimiter = Arc<RateLimiter<u64, DashMapStateStore<u64>, QuantaClock, NoOpMiddleware<QuantaInstant>>>;

/// A filter that checks for and validates a token.
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
}

// Note: Needs to be async for the `and_then` combinator
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
