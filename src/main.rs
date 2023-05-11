pub mod gateway;
pub mod models;

#[tokio::main]
async fn main() {
    let gateway_routes = gateway::handler::get_routes();

    warp::serve(gateway_routes)
        .run(([127, 0, 0, 1], 8080))
        .await;
}
