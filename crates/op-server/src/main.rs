//! OpenProject RS Server
//!
//! Production-ready HTTP server for OpenProject Rust implementation.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{middleware, routing::get, Json, Router};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use op_core::config::AppConfig;
use op_db::{Database, DatabaseConfig};

mod health;
mod metrics;

use health::{AppState, HealthChecker, HealthConfig};
use metrics::Metrics;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    init_tracing();

    // Load configuration
    dotenvy::dotenv().ok();
    let config = AppConfig::from_env().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config from env: {}, using defaults", e);
        AppConfig::default()
    });

    info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %config.server.host,
        port = config.server.port,
        "Starting OpenProject RS"
    );

    // Connect to database
    let db_config = DatabaseConfig::with_url(&config.database.url);
    let db = match Database::connect(&db_config).await {
        Ok(db) => {
            info!("Connected to database");

            // Apply schema migrations on boot. A fresh PaaS Postgres (Railway
            // etc.) starts empty, and every op-db query targets tables that
            // must already exist — so migrate-on-boot is what makes the
            // deploy self-hydrating. A migration failure is fatal: serving
            // against a half-migrated schema is worse than refusing to start.
            match db.run_migrations().await {
                Ok(()) => info!("Schema migrations applied"),
                Err(e) => {
                    tracing::error!("Migration failed: {e}");
                    return Err(e.into());
                }
            }

            // Reality-check seed: load the mock kanban board only when asked
            // (`HYDRATE=1`). Never seeds a real DB by default.
            if env_flag("HYDRATE") {
                match db.seed_kanban().await {
                    Ok(()) => info!("Kanban reality-check seed loaded (HYDRATE=1)"),
                    Err(e) => tracing::error!("Seed failed (continuing unseeded): {e}"),
                }
            }

            Some(db)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to connect to database: {}. Running without database.",
                e
            );
            None
        }
    };

    // Initialize components
    let metrics = Arc::new(Metrics::new());
    let mut health_checker = HealthChecker::new(HealthConfig::default());
    if let Some(ref db) = db {
        health_checker = health_checker.with_pool(db.pool().clone());
    }

    let app_state = Arc::new(AppState {
        health: Arc::new(health_checker),
        config: config.clone(),
        db: db.map(|d| d.pool().clone()),
    });

    // Build router
    let app = build_router(app_state.clone(), metrics.clone());

    // Start server — honor $PORT for PaaS deploys (Railway / Heroku / Cloud Run /
    // Fly route their public edge to $PORT; the app must bind 0.0.0.0:$PORT or the
    // proxy can't reach it). Falls back to the configured host:port for local /
    // non-PaaS deploys. See lance-graph CONSUMER_SCAN_TODO §B1 (canonical pattern
    // landed in medcare-server first).
    let addr = resolve_bind_addr(std::env::var("PORT").ok(), config.server_addr())?;
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

/// A boolean env var is "on" for `1` / `true` / `yes` (case-insensitive,
/// trimmed); anything else — including unset — is off. Used for the
/// `HYDRATE` reality-check seed gate.
fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref().map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes")
    )
}

/// Resolve the bind address from the `$PORT` env var (when set) or the
/// configured fallback. PaaS proxies (Railway / Heroku / Cloud Run / Fly) route
/// the public edge to `$PORT` and require `0.0.0.0` so the proxy can reach the
/// container; non-PaaS deploys keep the configured host:port. Whitespace-only
/// `$PORT` values are treated as unset.
///
/// Pure helper — extracted so the parse logic is testable without touching
/// process env (the crate forbids unsafe; `std::env::set_var` is unsafe in
/// recent Rust). Tests cover the four input shapes (set/empty/whitespace/
/// malformed) directly.
///
/// See lance-graph `CONSUMER_SCAN_TODO.md` §B1.
fn resolve_bind_addr(env_port: Option<String>, fallback: SocketAddr) -> anyhow::Result<SocketAddr> {
    match env_port.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => format!("0.0.0.0:{p}")
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid PORT env var `{p}`: {e}")),
        _ => Ok(fallback),
    }
}

/// Initialize tracing/logging
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,op_server=debug,op_api=debug,tower_http=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .init();
}

/// Build the application router
fn build_router(state: Arc<AppState>, metrics: Arc<Metrics>) -> Router {
    // Health check routes (no auth required)
    let health_routes = Router::new()
        // A friendly root instead of a bare 404 — this is an API server, so `/`
        // points the visitor at the live surface (the domain root in a browser
        // used to just 404).
        .route("/", get(root_landing))
        .route("/health", get(health::default_health_check))
        .route("/health_checks/default", get(health::default_health_check))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/health/full", get(health::health))
        .with_state(state.clone());

    // Metrics routes
    let metrics_routes = Router::new()
        .route("/metrics", get(metrics::prometheus_metrics))
        .route("/metrics.json", get(metrics::json_metrics))
        .with_state(metrics.clone());

    // The full OpenProject `/api/v3` surface (op-api): work_packages, projects,
    // users, statuses, versions, configuration, … — real handlers that query
    // the DB pool via repositories. op-api is the SINGLE owner of the
    // `/api/v3` namespace (5+3 council, R2: a second router nesting the same
    // prefix panics at startup the day the route sets overlap). op-api
    // carries its own `AppState`, so bridge op-server's pool into it here.
    //
    // base_url: prefer the deploy's PUBLIC address (`PUBLIC_URL`, or Railway's
    // injected `RAILWAY_PUBLIC_DOMAIN`) — the bind host is `0.0.0.0` on PaaS
    // and must never leak into HAL links (council R1/R2).
    let base_url = std::env::var("PUBLIC_URL")
        .ok()
        .or_else(|| std::env::var("RAILWAY_PUBLIC_DOMAIN").ok().map(|d| format!("https://{d}")))
        .unwrap_or_else(|| {
            format!("http://{}:{}", state.config.server.host, state.config.server.port)
        });
    let openproject_api = op_api::routes::router().with_state(op_api::extractors::AppState {
        config: Arc::new(op_api::extractors::AppConfig {
            api_version: "3".to_string(),
            base_url,
            // SECURE BY DEFAULT (5+3 council, S2-P0): authentication is
            // required unless the operator EXPLICITLY opens the instance with
            // `OP_ALLOW_ANONYMOUS=1` (the reality-check/demo posture: anonymous
            // reads AND writes on the mock board). The old hardcoded `false`
            // meant every deploy shipped an anonymous write surface and only a
            // code change + rebuild could close it. Future single source:
            // derive this from op-core's `AuthConfig` when real auth lands.
            require_authentication: !env_flag("OP_ALLOW_ANONYMOUS"),
        }),
        db: state.db.clone(),
    });

    // Main router
    Router::new()
        .merge(health_routes)
        .merge(metrics_routes)
        .merge(openproject_api)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                ),
        )
        .layer(middleware::from_fn_with_state(
            metrics,
            metrics::metrics_middleware,
        ))
}

/// Root landing — an API server has no HTML site, so `/` used to 404 in a
/// browser (looked broken). Point the visitor at the live surface instead.
async fn root_landing() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "openproject-rs",
        "version": env!("CARGO_PKG_VERSION"),
        "_links": {
            "api": { "href": "/api/v3" },
            "health": { "href": "/health" },
            "ready": { "href": "/health/ready" },
            "workPackages": { "href": "/api/v3/work_packages" },
            "projects": { "href": "/api/v3/projects" }
        }
    }))
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_app() -> Router {
        let metrics = Arc::new(Metrics::new());
        let health_checker = Arc::new(HealthChecker::new(HealthConfig::default()));
        let config = AppConfig::default();

        let state = Arc::new(AppState {
            health: health_checker,
            config,
            db: None,
        });

        build_router(state, metrics)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_root() {
        let app = test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let app = test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // ── PaaS deploy: $PORT bind (CONSUMER_SCAN_TODO §B1) ────────────────

    fn fallback_addr() -> SocketAddr {
        // Stand-in for `config.server_addr()` in the no-env-override branch;
        // mirrors the default in `AppConfig::default()` (port 8080).
        "127.0.0.1:8080".parse().unwrap()
    }

    #[test]
    fn resolve_bind_addr_uses_port_env_when_set() {
        // The PaaS case: proxy routes to $PORT → bind 0.0.0.0:$PORT so it
        // can be reached. Host fixed to all-interfaces, port from the env.
        let addr = resolve_bind_addr(Some("3000".into()), fallback_addr()).unwrap();
        assert_eq!(addr, "0.0.0.0:3000".parse().unwrap());
    }

    #[test]
    fn resolve_bind_addr_falls_back_when_port_env_is_unset() {
        // Local / non-PaaS: $PORT unset, use the configured host:port.
        let addr = resolve_bind_addr(None, fallback_addr()).unwrap();
        assert_eq!(addr, fallback_addr());
    }

    #[test]
    fn resolve_bind_addr_treats_empty_or_whitespace_port_as_unset() {
        // Whitespace-only values (a common docker-compose / env-file
        // mistake) should not be parsed as a port; fall back instead of
        // failing the boot.
        for empty in ["", " ", "\t", " \n "] {
            let addr = resolve_bind_addr(Some(empty.into()), fallback_addr()).unwrap();
            assert_eq!(addr, fallback_addr(), "input {empty:?} should fall back");
        }
    }

    #[test]
    fn resolve_bind_addr_rejects_malformed_port_with_diagnostic() {
        // A misconfigured $PORT must surface as a boot error naming the bad
        // value, not silently fall back (which would hide the bug).
        for bad in ["abc", "70000", "-1", "8080:extra"] {
            let err = resolve_bind_addr(Some(bad.into()), fallback_addr()).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("invalid PORT") && msg.contains(bad),
                "input {bad:?} should yield a diagnostic naming the value; got {msg}"
            );
        }
    }
}
