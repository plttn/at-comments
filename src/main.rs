mod models;
mod rss_poller;
mod settings;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use env_logger::{Builder, Target};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;

// use crate::settings::Settings;

#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
}

// use config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stdout);
    builder.init();

    let config = settings::build_config()?;

    // Create database pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.get_string("database.url")?)
        .await?;

    log::info!("Connected to database");

    let app_state = AppState { pool: pool.clone() };

    // Spawn background RSS poller task
    let poller_pool = pool.clone();
    tokio::spawn(async move {
        rss_poller::rss_polling_task(poller_pool).await;
    });

    // Build router
    let app = Router::new()
        .route("/", get(index))
        .route("/slug", get(slug_root))
        .route("/slug/:slug", get(post_meta))
        .fallback(not_found)
        .with_state(app_state);

    // Bind and serve
    let address = config.get_string("app.address")?;
    let port = config.get_int("app.port")? as u16;
    let addr = format!("{}:{}", address, port).parse::<SocketAddr>()?;
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

async fn slug_root() -> Json<Value> {
    Json(json!({
        "status": "fail",
        "data": {"slug": "A slug is required: /slug/<slug>"}
    }))
}

async fn post_meta(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let result = sqlx::query("SELECT id, slug, rkey, time_us FROM posts WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&state.pool)
        .await;

    match result {
        Ok(row) => {
            let meta = models::Meta {
                id: row.get(0),
                slug: row.get(1),
                rkey: row.get(2),
                time_us: row.get(3),
            };
            Ok(Json(json!({
                "status": "success",
                "data": {"post": meta}
            })))
        }
        Err(_) => {
            // Not in DB — check the live RSS feed
            match rss_poller::lookup_slug_in_rss(&slug).await {
                Some((rkey, time_us)) => {
                    // Insert; ignore conflicts in case the background poller raced us
                    let _ = sqlx::query(
                        "INSERT INTO posts (slug, rkey, time_us) VALUES ($1, $2, $3) ON CONFLICT (slug) DO NOTHING"
                    )
                    .bind(&slug)
                    .bind(&rkey)
                    .bind(&time_us)
                    .execute(&state.pool)
                    .await;

                    let meta = models::Meta {
                        id: 0, // Will be fetched from DB on next request
                        slug,
                        rkey,
                        time_us,
                    };
                    Ok(Json(json!({
                        "status": "success",
                        "data": {"post": meta}
                    })))
                }
                None => Err(AppError::NotFound),
            }
        }
    }
}

enum AppError {
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "status": "fail",
                    "data": {"message": "Post not found"}
                })),
            )
                .into_response(),
        }
    }
}

async fn not_found() -> (StatusCode, String) {
    (
        StatusCode::NOT_FOUND,
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
