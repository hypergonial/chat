pub mod gateway;
pub mod macros;
pub mod models;
pub mod rest;
use models::appstate::APP;
use warp::Filter;

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::fmt().compact().with_target(false).without_time().finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    let gateway_routes = gateway::handler::get_routes();
    let rest_routes = rest::routes::get_routes();

    // Initialize the database
    if let Err(e) = APP.write().await.init().await {
        tracing::error!(message = "Failed initializing application", error = %e);
        return;
    }

    warp::serve(gateway_routes.or(rest_routes))
        .run(APP.read().await.config().listen_addr())
        .await;
}
