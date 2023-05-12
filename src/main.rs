pub mod gateway;
pub mod models;
pub mod rest;
use warp::Filter;

#[tokio::main]
async fn main() {
    let gateway_routes = gateway::handler::get_routes();
    let rest_routes = rest::routes::get_routes();

    warp::serve(gateway_routes.or(rest_routes))
        .run(([127, 0, 0, 1], 8080))
        .await;
}
