use std::convert::Infallible;
use std::error::Error;

use crate::models::rejections::{
    BadRequest, ErrorMessage, Forbidden, InternalServerError, NotFound, RateLimited, Unauthorized,
};
use warp::http::StatusCode;
use warp::{Rejection, Reply};

/// Handle a rejection and return a JSON response
///
/// # Parameters
///
/// * `err` - The rejection to handle
///
/// # Returns
///
/// A JSON response with the appropriate status code.
pub async fn handle_rejection(err: Rejection) -> Result<Box<dyn Reply>, Infallible> {
    let code;
    let message;
    let mut description = None;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT_FOUND";
        description = Some("The requested resource could not be found.".into());
    } else if let Some(e) = err.find::<NotFound>() {
        code = StatusCode::NOT_FOUND;
        message = "NOT_FOUND";
        description = Some(format!("Not Found: {}", e.message));
    // Unauthorized rejections are handled separately to allow for the `WWW-Authenticate` header
    } else if let Some(e) = err.find::<Unauthorized>() {
        code = StatusCode::UNAUTHORIZED;
        message = "UNAUTHORIZED";
        description = Some(format!("Unauthorized: {}", e.message));

        let json = warp::reply::json(&ErrorMessage::new(code.into(), message.into(), description));
        return Ok(Box::new(warp::reply::with_header(
            warp::reply::with_status(json, code),
            "WWW-Authenticate",
            e.header.clone(),
        )));
    } else if let Some(e) = err.find::<BadRequest>() {
        code = StatusCode::BAD_REQUEST;
        message = "BAD_REQUEST";
        description = Some(format!("Bad Request: {}", e.message));
    } else if let Some(e) = err.find::<RateLimited>() {
        code = StatusCode::TOO_MANY_REQUESTS;
        message = "TOO_MANY_REQUESTS";
        description = Some(format!("Rate Limited: {}", e.message));
    } else if let Some(e) = err.find::<Forbidden>() {
        code = StatusCode::FORBIDDEN;
        message = "FORBIDDEN";
        description = Some(format!("Forbidden: {}", e.message));
    } else if let Some(e) = err.find::<warp::reject::MissingHeader>() {
        code = StatusCode::BAD_REQUEST;
        message = "BAD_REQUEST";
        description = Some(format!("Missing Header: {}", e.name()));
    } else if let Some(e) = err.find::<warp::reject::InvalidHeader>() {
        code = StatusCode::BAD_REQUEST;
        message = "BAD_REQUEST";
        description = Some(format!("Invalid Header: {}", e.name()));
    } else if let Some(e) = err.find::<warp::reject::PayloadTooLarge>() {
        code = StatusCode::PAYLOAD_TOO_LARGE;
        message = "PAYLOAD_TOO_LARGE";
        description = Some(e.to_string());
    } else if let Some(e) = err.find::<warp::filters::body::BodyDeserializeError>() {
        message = "BAD_REQUEST";
        description = e.source().map(|e| e.to_string());
        code = StatusCode::BAD_REQUEST;
    } else if let Some(e) = err.find::<warp::reject::MethodNotAllowed>() {
        message = "METHOD_NOT_ALLOWED";
        description = Some(e.to_string());
        code = StatusCode::METHOD_NOT_ALLOWED;
    } else if let Some(e) = err.find::<InternalServerError>() {
        tracing::error!("Internal Server Error: {}", e.message);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "INTERNAL_SERVER_ERROR";
        description = Some(format!("Internal Server Error: {}", e.message));
    } else {
        tracing::error!("Unhandled rejection: {:?}", err);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "INTERNAL_SERVER_ERROR";
    }

    let json = warp::reply::json(&ErrorMessage::new(code.into(), message.into(), description));

    Ok(Box::new(warp::reply::with_status(json, code)))
}
