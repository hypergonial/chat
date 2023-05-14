pub mod gateway;
pub mod macros;
pub mod models;
pub mod rest;
use std::process::ExitCode;

use models::appstate::APP;
use tokio::signal::ctrl_c;
use tokio::signal::unix::{signal, SignalKind};
use warp::Filter;

async fn handle_signals() {
    let mut sigterm =
        signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal listener");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, terminating...");
        }
        _ = ctrl_c() => {
            tracing::info!("Received keyboard interrupt, terminating...");
        }
    };
}

#[tokio::main]
async fn main() -> ExitCode {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .without_time()
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    let gateway_routes = gateway::handler::get_routes();
    let rest_routes = rest::routes::get_routes();

    // Initialize the database
    if let Err(e) = APP.write().await.init().await {
        tracing::error!(message = "Failed initializing application", error = %e);
        return ExitCode::FAILURE;
    }

    tokio::select!(
        _ = handle_signals() => {},
        _ = warp::serve(gateway_routes.or(rest_routes))
            .run(APP.read().await.config().listen_addr()) => {}
    );
    APP.write().await.close().await;

    ExitCode::SUCCESS
}
