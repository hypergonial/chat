pub mod gateway;
pub mod models;
pub mod rest;
use crate::models::db::DB;
use dotenv::dotenv;
use std::env;
use warp::Filter;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let gateway_routes = gateway::handler::get_routes();
    let rest_routes = rest::routes::get_routes();

    // Initialize the database
    DB.write()
        .await
        .connect(&env::var("DATABASE_URL").expect("DATABASE_URL must be set."))
        .await
        .expect("Failed to connect to database.");

    warp::serve(gateway_routes.or(rest_routes))
        .run(([127, 0, 0, 1], 8080))
        .await;
}
