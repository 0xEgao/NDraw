use std::sync::Arc;

use anyhow::Context;
use ndraw_server::{AppState, ServerConfig, build_router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing()?;
    let config = ServerConfig::from_env().context("invalid server configuration")?;
    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .with_context(|| format!("failed to bind {}", config.bind_address))?;
    let state = Arc::new(AppState::new(config));
    let app = build_router(Arc::clone(&state));

    tracing::info!(address = %state.config().bind_address, "NDraw server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state))
        .await
        .context("HTTP server failed")
}

fn init_tracing() -> anyhow::Result<()> {
    let filter = match tracing_subscriber::EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => tracing_subscriber::EnvFilter::new("ndraw_server=info,tower_http=info"),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .try_init()
        .map_err(|error| anyhow::anyhow!("failed to install tracing subscriber: {error}"))?;
    Ok(())
}

async fn shutdown_signal(state: Arc<AppState>) {
    let control_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = control_c => {}
        () = terminate => {}
    }
    tracing::info!("graceful shutdown started");
    state.shutdown().await;
}
