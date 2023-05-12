use crate::gateway::handler::GATEWAY;
use crate::models::{gateway_event::GatewayEvent, message::Message};
use std::time::Duration;
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
        .and(warp::body::json())
        .and_then(|msg| create_message(msg))
        .with(cors);

    create_msg.boxed()
}

async fn create_message(message: Message) -> Result<impl warp::Reply, warp::Rejection> {
    println!("Received message: {:?}", message);
    GATEWAY
        .read()
        .await
        .dispatch(message.author.id(), GatewayEvent::MessageCreate(message));
    Ok(warp::reply())
}
