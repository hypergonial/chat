pub mod gateway;
pub mod macros;
pub mod models;
pub mod rest;

use axum::Router;
use models::appstate::APP;
use tokio::signal::ctrl_c;
use tracing::level_filters::LevelFilter;
use color_eyre::eyre::Result;
use tower_http::trace::TraceLayer;

#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

#[cfg(unix)]
async fn handle_signals() {
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal listener");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, terminating...");
        }
        _ = ctrl_c() => {
            tracing::info!("Received keyboard interrupt, terminating...");
        }
    };
    APP.close().await;
}

#[cfg(not(unix))]
async fn handle_signals() {
    ctrl_c().await.expect("Failed to create CTRL+C signal listener");
    APP.close().await;
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    #[cfg(debug_assertions)]
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .with_max_level(LevelFilter::DEBUG)
        .without_time()
        .finish();

    #[cfg(not(debug_assertions))]
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .without_time()
        .finish();

    /* console_subscriber::init(); */
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    let gateway_routes = gateway::handler::get_router();
    let rest_routes = rest::routes::get_router();

    // Initialize the database
    APP.init().await?;

    let app = Router::new()
        .nest("/gateway/v1", gateway_routes)
        .nest("/api/v1", rest_routes)
        .layer(TraceLayer::new_for_http());

    tracing::info!("Listening on {}", APP.config().listen_addr());

    hyper::Server::bind(&APP.config().listen_addr())
        .serve(app.into_make_service())
        .with_graceful_shutdown(handle_signals())
        .await
        .expect("Failed creating server");

    Ok(())
}
