#![feature(exit_status_error)]

mod bisect;
mod db;

use std::sync::{mpsc, Arc, Mutex};

use crate::bisect::Job;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post, Router},
    Extension, Json,
};
use color_eyre::eyre::Context;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::env;
use tower_http::trace::TraceLayer;
use tracing::{error, info, metadata::LevelFilter};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

type SendChannel = Arc<Mutex<mpsc::SyncSender<Job>>>;
type Conn = Arc<Mutex<Connection>>;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let (job_queue_send, job_queue_recv) = mpsc::sync_channel(10);

    let sqlite_db = env::var("SQLITE_DB").unwrap_or_else(|_| "bisect.sqlite".to_string());

    let main_conn = Connection::open(&sqlite_db)
        .wrap_err_with(|| format!("connect to sqlite with file path: {}", sqlite_db))?;
    let main_conn = Arc::new(Mutex::new(main_conn));

    let worker_conn = Connection::open(&sqlite_db)
        .wrap_err_with(|| format!("connect to sqlite with file path: {}", sqlite_db))?;

    db::setup(&worker_conn).wrap_err("db setup")?;

    let app = Router::new()
        .route("/", get(|| async { index_html() }))
        .route("/bisect/:id", get(get_bisection))
        .route("/bisect", get(get_bisections))
        .route("/bisect", post(do_bisection))
        // this is really stupid and hacky
        .layer(Extension(Arc::new(Mutex::new(job_queue_send))))
        .layer(Extension(main_conn))
        .layer(TraceLayer::new_for_http());

    std::thread::spawn(|| bisect::bisect_worker(job_queue_recv, worker_conn));

    info!("Starting up server on port 4000");

    axum::Server::bind(&"0.0.0.0:4000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .wrap_err("failed to start server")
}

fn index_html() -> impl IntoResponse {
    Html(include_str!("../index.html"))
}

async fn get_bisection(
    Extension(conn): Extension<Conn>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    db::get_bisection(&conn.lock().unwrap(), id)
        .map(|bisections| Json(bisections))
        .map_err(|err| {
            error!(?err, "error getting bisections");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })
}

async fn get_bisections(Extension(conn): Extension<Conn>) -> impl IntoResponse {
    db::get_bisections(&conn.lock().unwrap())
        .map(|bisections| Json(bisections))
        .map_err(|err| {
            error!(?err, "error getting bisections");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })
}

#[derive(Debug, Deserialize)]
pub struct Options {
    start: chrono::NaiveDate,
    end: Option<chrono::NaiveDate>,
    kind: Option<String>,
}

#[derive(Debug, Serialize)]
struct JobIdReturn {
    job_id: Uuid,
}

async fn do_bisection(
    options: Query<Options>,
    body: String,
    send_channel: Extension<SendChannel>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4();

    let job = Job::new(job_id, body, options.0);

    match send_channel.0.lock().unwrap().send(job) {
        Ok(_) => {
            info!(id = %job_id, "Added new job to queue");
            Ok(Json(JobIdReturn { job_id }))
        }
        Err(_) => Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Too many jobs in the queue already",
        )),
    }
}
