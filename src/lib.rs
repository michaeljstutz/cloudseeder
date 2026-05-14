use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde_json::json;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;

pub mod config;

pub use config::{LoadError, PrefixSource, Settings};

pub fn app(prefix: &str) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route(&format!("/{prefix}/"), get(prefix_root))
        .fallback(unauthorized)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn prefix_root() -> impl IntoResponse {
    StatusCode::OK
}

async fn unauthorized() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, "")
}

pub async fn serve(addr: SocketAddr, prefix: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_with_shutdown(listener, prefix, shutdown_signal()).await
}

pub async fn serve_with_shutdown<F>(
    listener: TcpListener,
    prefix: &str,
    shutdown: F,
) -> std::io::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let bound = listener.local_addr()?;
    tracing::info!(%bound, "cloudseeder listening");
    axum::serve(listener, app(prefix))
        .with_graceful_shutdown(shutdown)
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
