use std::convert::Infallible;
use std::error::Error;

use crate::models::rejections::{BadRequest, ErrorMessage, Unauthorized, InternalServerError};
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
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message;
    let mut description = None;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT_FOUND";
        description = Some("The requested resource could not be found.".into());
    } else if let Some(e) = err.find::<Unauthorized>() {
        code = StatusCode::UNAUTHORIZED;
        message = "UNAUTHORIZED";
        description = Some(format!("Unauthorized: {}", e.message));
    } else if let Some(e) = err.find::<BadRequest>() {
        code = StatusCode::BAD_REQUEST;
        message = "BAD_REQUEST";
        description = Some(format!("Bad Request: {}", e.message));
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
        eprintln!("Internal Server Error: {}", e.message);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "INTERNAL_SERVER_ERROR";
        description = Some(format!("Internal Server Error: {}", e.message));
    } else {
        eprintln!("Unhandled rejection: {:?}", err);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "INTERNAL_SERVER_ERROR";
    }

    let json = warp::reply::json(&ErrorMessage::new(code.into(), message.into(), description));

    Ok(warp::reply::with_status(json, code))
}
