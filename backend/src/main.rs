mod adapters;
mod engine;
mod formats;
mod jobs;
mod output;
mod routes;

use crate::engine::ConversionEngine;
use crate::jobs::JobStore;
use crate::output::OutputManager;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;


#[derive(Clone)]
pub struct AppState {
    pub jobs: JobStore,
    pub output: OutputManager,
    pub engine: ConversionEngine,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "convert_anything=info,tower_http=info".into()),
        )
        .init();

    let data_dir = std::env::var("CONVERT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("convert"));
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);

    tokio::fs::create_dir_all(&data_dir)
        .await
        .expect("create data dir");

    let jobs = JobStore::new();
    let output = OutputManager::new(data_dir);
    let rayon_pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus_fallback())
            .thread_name(|i| format!("convert-rayon-{i}"))
            .build()
            .expect("rayon pool"),
    );
    let engine = ConversionEngine::new(jobs.clone(), output.clone(), rayon_pool);

    spawn_cleanup(jobs.clone(), output.clone());

    let state = AppState {
        jobs,
        output,
        engine,
    };

    let app = Router::new()
        .route("/api/formats", get(routes::list_formats))
        .route("/api/convert", post(routes::convert))
        .route("/api/jobs/{id}", get(routes::get_job))
        .route("/api/jobs/{id}/download", get(routes::download))
        .layer(DefaultBodyLimit::disable())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "listening");
    let listener = TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("server");
}

fn spawn_cleanup(jobs: JobStore, output: OutputManager) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            let dirs = jobs.remove_expired(chrono::Duration::minutes(30)).await;
            for dir in dirs {
                output.cleanup_dir(&dir).await;
            }
        }
    });
}

fn num_cpus_fallback() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
