#![allow(async_fn_in_trait)]

pub mod gateway;
pub mod macros;
pub mod models;
pub mod rest;

use axum::Router;
use color_eyre::eyre::Result;
use models::appstate::app;
use tokio::signal::ctrl_c;
use tower_http::trace::TraceLayer;

#[cfg(debug_assertions)]
use tracing::level_filters::LevelFilter;

#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

use crate::models::appstate::{ApplicationState, APP};

#[cfg(unix)]
async fn handle_signals() {
    use std::process::exit;

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal listener");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, terminating...");
        }
        _ = ctrl_c() => {
            tracing::info!("Received keyboard interrupt, terminating...");
        }
    };
    APP.get().unwrap_or_else(|| exit(1)).close().await;
}

#[cfg(not(unix))]
async fn handle_signals() {
    ctrl_c().await.expect("Failed to create CTRL+C signal listener");
    app().close().await;
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

    // Initialize the application state
    let mut state = ApplicationState::new();
    state.init().await?;
    APP.set(state)
        .unwrap_or_else(|_| panic!("Failed to set application state"));

    let router = Router::new()
        .nest("/gateway/v1", gateway_routes)
        .nest("/api/v1", rest_routes)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(app().config.listen_addr())
        .await
        .expect("Failed to bind to address");

    tracing::info!("Listening on {}", app().config.listen_addr());

    axum::serve(listener, router)
        .with_graceful_shutdown(handle_signals())
        .await
        .expect("Failed creating server");

    Ok(())
}
