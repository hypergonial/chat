pub mod gateway;
pub mod macros;
pub mod models;
pub mod rest;
use models::appstate::APP;
use warp::Filter;

#[tokio::main]
async fn main() {
    let gateway_routes = gateway::handler::get_routes();
    let rest_routes = rest::routes::get_routes();

    // Initialize the database
    if let Err(e) = APP.write().await.init().await {
        eprintln!("Failed initializing application: {}", e);
        return;
    }

    warp::serve(gateway_routes.or(rest_routes))
        .run(([127, 0, 0, 1], 8080))
        .await;
}
