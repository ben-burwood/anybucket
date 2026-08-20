mod config;
mod error;
mod handlers;
mod secret_store;

use std::sync::Arc;

use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::http::Request;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::Mutex;
use tower::service_fn;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use anybucket_core::connections::ConnectionStore;
use anybucket_core::state::AppState;

use config::Config;
use handlers::SharedState;
use secret_store::FileSecretStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "anybucket_server=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env();

    let secrets = FileSecretStore::new(&config.config_dir)?;
    let store = ConnectionStore::load(&config.config_dir, Box::new(secrets))?;
    let state: SharedState = Arc::new(Mutex::new(AppState::new(store)));

    let app = build_router(state, &config);

    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!(
        "anybucket-server listening on http://{} (serving {})",
        config.addr,
        config.static_dir.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(state: SharedState, config: &Config) -> Router {
    let api = Router::new()
        .route("/health", get(handlers::health))
        // Connection management
        .route("/list_connections", post(handlers::list_connections))
        .route(
            "/get_active_connection",
            post(handlers::get_active_connection),
        )
        .route("/save_connection", post(handlers::save_connection))
        .route("/delete_connection", post(handlers::delete_connection))
        .route(
            "/set_active_connection",
            post(handlers::set_active_connection),
        )
        .route("/test_connection", post(handlers::test_connection))
        // Browsing (read-only)
        .route("/list_buckets", post(handlers::list_buckets))
        // Bucket administration (admin connections only)
        .route("/create_bucket", post(handlers::create_bucket))
        .route("/delete_bucket", post(handlers::delete_bucket))
        .route("/list_objects", post(handlers::list_objects))
        .route("/head_object", post(handlers::head_object))
        .route("/presign_get", post(handlers::presign_get))
        .route("/object_uris", post(handlers::object_uris))
        .route("/object_exists", post(handlers::object_exists))
        .route("/create_folder", post(handlers::create_folder))
        // Streaming (NDJSON progress)
        .route("/delete_objects", post(handlers::delete_objects))
        .route("/transfer_objects", post(handlers::transfer_objects))
        .route("/scan_bucket_metrics", post(handlers::scan_bucket_metrics))
        // Raw-body upload + streamed download (browser filesystem reroute)
        .route("/objects/upload", post(handlers::upload_object))
        .route("/objects/download", get(handlers::download_object));

    // Serve the built SPA; unknown paths fall back to index.html so client-side routing works on deep links / refreshes.
    // The fallback returns index.html with a 200 (ServeDir's own not-found path would serve it with a 404, which
    // makes deep links look like errors), so a refreshed deep link loads cleanly.
    // Read once at startup into `Bytes` so each fallback hit clones a cheap
    // refcounted handle rather than copying the whole HTML body.
    let index_html: Bytes = std::fs::read_to_string(config.static_dir.join("index.html"))
        .unwrap_or_else(|_| "<!doctype html><title>AnyBucket</title>".to_string())
        .into();
    let spa_fallback = service_fn(move |_req: Request<Body>| {
        let html = index_html.clone();
        async move { Ok::<_, Infallible>(Html(html).into_response()) }
    });
    // `fallback` (not `not_found_service`) preserves the fallback's 200 status;
    // `not_found_service` would force a 404 even though we serve index.html.
    let serve_dir = ServeDir::new(&config.static_dir).fallback(spa_fallback);

    Router::new()
        .nest("/api", api)
        .fallback_service(serve_dir)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
