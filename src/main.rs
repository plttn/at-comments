mod models;
mod rss_poller;
mod settings;

use axum::{
    extract::{Path, State},
    http::{
        header::{self},
        HeaderMap, StatusCode,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use env_logger::{Builder, Target};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
    client: reqwest::Client,
    poller_config: settings::PollerConfig,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stdout);
    builder.init();

    let config = settings::build_config()?;

    // Create database pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database.url)
        .await?;

    log::info!("Connected to database");

    sqlx::migrate!().run(&pool).await?;
    log::info!("Database migrations applied");

    // Create shared HTTP client with a request timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let app_state = AppState {
        pool: pool.clone(),
        client: client.clone(),
        poller_config: config.poller.clone(),
    };

    // Spawn background RSS poller task
    let poller_pool = pool.clone();
    let poller_config = config.poller.clone();
    tokio::spawn(async move {
        rss_poller::rss_polling_task(client, poller_pool, poller_config).await;
    });

    // Build router
    let app = Router::new()
        .route("/", get(index))
        .route("/slug", get(slug_root))
        .route("/slug/{slug}", get(post_meta))
        .fallback(not_found)
        .with_state(app_state);

    // Bind and serve
    let addr = format!("{}:{}", config.app.address, config.app.port).parse::<SocketAddr>()?;
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn index() -> &'static str {
    r##"
        __                                                         __
       /\ \__                                                     /\ \__
   __  \ \ ,_\   ___    ___     ___ ___     ___ ___      __    ___\ \ ,_\   ____
 /'__`\ \ \ \/  /'___\ / __`\ /' __` __`\ /' __` __`\  /'__`\/' _ `\ \ \/  /',__\
/\ \L\.\_\ \ \_/\ \__//\ \L\ \/\ \/\ \/\ \/\ \/\ \/\ \/\  __//\ \/\ \ \ \_/\__, `\
\ \__/.\_\\ \__\ \____\ \____/\ \_\ \_\ \_\ \_\ \_\ \_\ \____\ \_\ \_\ \__\/\____/
 \/__/\/_/ \/__/\/____/\/___/  \/_/\/_/\/_/\/_/\/_/\/_/\/____/\/_/\/_/\/__/\/___/

    at-comments API server.
    "##
}

async fn slug_root() -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "status": "fail",
            "data": {"slug": "A slug is required: /slug/<slug>"}
        })),
    )
}

async fn post_meta(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let result = sqlx::query("SELECT slug, rkey FROM posts WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&state.pool)
        .await;

    match result {
        Ok(row) => {
            let meta = models::Meta {
                slug: row.get(0),
                rkey: row.get(1),
            };
            Ok(Json(json!({
                "status": "success",
                "data": {"post": meta}
            })))
        }
        Err(sqlx::Error::RowNotFound) => {
            // Not in DB — check the live RSS feed
            match rss_poller::lookup_slug_in_rss(&state.client, &slug, &state.poller_config).await {
                Some((rkey, time_us)) => {
                    // Insert; ignore conflicts in case the background poller raced us
                    let _ = sqlx::query(
                        "INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING"
                    )
                    .bind(&slug)
                    .bind(&rkey)
                    .bind(time_us)
                    .execute(&state.pool)
                    .await;

                    Ok(Json(json!({
                        "status": "success",
                        "data": {"post": models::Meta { slug, rkey }}
                    })))
                }
                None => Err(AppError::NotFound),
            }
        }
        Err(e) => {
            log::error!("Database error looking up slug '{}': {}", slug, e);
            Err(AppError::DatabaseError)
        }
    }
}

enum AppError {
    NotFound,
    DatabaseError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::NotFound => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-store"),
                );
                (
                    StatusCode::NOT_FOUND,
                    headers,
                    Json(json!({
                        "status": "fail",
                        "data": {"message": "Post not found"}
                    })),
                )
                    .into_response()
            }
            AppError::DatabaseError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": "An internal error occurred"
                })),
            )
                .into_response(),
        }
    }
}

async fn not_found() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    (
        StatusCode::NOT_FOUND,
        headers,
        "Sorry, that path is not valid.".to_string(),
    )
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
