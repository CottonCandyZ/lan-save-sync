use std::{io, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::TryStreamExt;
use serde::Deserialize;
use tempfile::NamedTempFile;
use tokio_util::io::{ReaderStream, StreamReader};
use tower_http::trace::TraceLayer;

use crate::{
    archive,
    config::find_folder,
    engine::Engine,
    manifest,
    model::{AckRequest, ApiError, ApplyResult, FolderInfo, InfoResponse, SyncAction},
};

#[derive(Clone)]
struct ServerState {
    engine: Engine,
    mutation_lock: Arc<tokio::sync::Mutex<()>>,
}

pub async fn serve(engine: Engine) -> Result<()> {
    let state = ServerState {
        engine,
        mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/info", get(info))
        .route("/v1/manifest", get(get_manifest))
        .route("/v1/archive", get(get_archive))
        .route("/v1/apply", post(post_apply))
        .route("/v1/ack", post(post_ack))
        .route("/v1/plan", get(local_plan))
        .route("/v1/sync", post(local_sync))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.engine.config.listen)
        .await
        .with_context(|| format!("failed to listen on {}", state.engine.config.listen))?;
    tracing::info!(
        device = %state.engine.config.device.id,
        listen = %state.engine.config.listen,
        "LAN Save Sync agent started"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn info(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> ApiResult<Json<InfoResponse>> {
    authorize(&state, &headers)?;
    Ok(Json(InfoResponse {
        device: state.engine.config.device.clone(),
        folders: state
            .engine
            .config
            .folders
            .iter()
            .map(|folder| FolderInfo {
                id: folder.id.clone(),
                name: folder.name.clone(),
                path: folder.path.clone(),
                enabled: folder.enabled,
            })
            .collect(),
    }))
}

#[derive(Deserialize)]
struct FolderQuery {
    folder_id: String,
}

async fn get_manifest(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<FolderQuery>,
) -> ApiResult<Json<crate::model::Manifest>> {
    authorize(&state, &headers)?;
    let folder = find_folder(&state.engine.config, &query.folder_id)?;
    Ok(Json(manifest::scan(folder)?))
}

#[derive(Deserialize)]
struct ArchiveQuery {
    folder_id: String,
    expected: String,
}

async fn get_archive(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<ArchiveQuery>,
) -> ApiResult<Response> {
    authorize(&state, &headers)?;
    let folder = find_folder(&state.engine.config, &query.folder_id)?.clone();
    let data_dir = state.engine.config.data_dir.clone();
    let expected = query.expected;
    let prepared = tokio::task::spawn_blocking(move || {
        archive::prepare_archive(&folder, Some(&expected), &data_dir)
    })
    .await
    .map_err(|error| anyhow::anyhow!("archive worker failed: {error}"))??;

    // Streaming response keeps memory usage bounded. Persist the temporary file;
    // the wrapper removes it after the response body is dropped.
    let (_file, path) = prepared.file.keep().map_err(|error| error.error)?;
    let file = tokio::fs::File::open(&path).await?;
    let stream = DeleteOnDropStream {
        inner: ReaderStream::new(file),
        path,
    };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/gzip")
        .body(Body::from_stream(stream))?)
}

#[derive(Deserialize)]
struct ApplyQuery {
    folder_id: String,
    expected_current: String,
    source_hash: String,
    source_device: String,
}

async fn post_apply(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<ApplyQuery>,
    body: Body,
) -> ApiResult<Json<ApplyResult>> {
    authorize(&state, &headers)?;
    crate::config::validate_id(&query.source_device, "source_device")?;
    let _guard = state.mutation_lock.lock().await;
    let temp = NamedTempFile::new_in(&state.engine.config.data_dir)?;
    let stream = body
        .into_data_stream()
        .map_err(|error| io::Error::other(error.to_string()));
    let mut reader = StreamReader::new(stream);
    let mut output = tokio::fs::File::create(temp.path()).await?;
    tokio::io::copy(&mut reader, &mut output).await?;
    tokio::io::AsyncWriteExt::flush(&mut output).await?;
    drop(output);
    let _operation_lock =
        crate::operation_lock::OperationLock::acquire(&state.engine.config.data_dir)?;

    let folder = find_folder(&state.engine.config, &query.folder_id)?.clone();
    let archive_path = temp.path().to_path_buf();
    let data_dir = state.engine.config.data_dir.clone();
    let source_hash = query.source_hash.clone();
    let expected = query.expected_current.clone();
    let history_limit = state.engine.config.history_limit;
    let result = tokio::task::spawn_blocking(move || {
        archive::apply_archive(
            &folder,
            &archive_path,
            &source_hash,
            Some(&expected),
            &data_dir,
            history_limit,
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("apply worker failed: {error}"))??;
    state
        .engine
        .state
        .set_base(&query.folder_id, &query.source_device, &result.root_hash)?;
    Ok(Json(result))
}

async fn post_ack(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<AckRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    authorize(&state, &headers)?;
    find_folder(&state.engine.config, &request.folder_id)?;
    crate::config::validate_id(&request.peer_id, "peer_id")?;
    state
        .engine
        .state
        .set_base(&request.folder_id, &request.peer_id, &request.root_hash)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
struct PlanQuery {
    peer_id: String,
    folder_id: String,
}

async fn local_plan(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<PlanQuery>,
) -> ApiResult<Json<crate::model::SyncPlan>> {
    authorize(&state, &headers)?;
    Ok(Json(
        state.engine.plan(&query.peer_id, &query.folder_id).await?,
    ))
}

#[derive(Deserialize)]
struct SyncRequest {
    peer_id: String,
    folder_id: String,
    action: SyncAction,
    #[serde(default)]
    accept_conflict: bool,
}

async fn local_sync(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<SyncRequest>,
) -> ApiResult<Json<ApplyResult>> {
    authorize(&state, &headers)?;
    let _guard = state.mutation_lock.lock().await;
    Ok(Json(
        state
            .engine
            .sync(
                &request.peer_id,
                &request.folder_id,
                request.action,
                request.accept_conflict,
            )
            .await?,
    ))
}

fn authorize(state: &ServerState, headers: &HeaderMap) -> ApiResult<()> {
    let expected = format!("Bearer {}", state.engine.config.api_token);
    let actual = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if actual != Some(expected.as_str()) {
        return Err(ApiResponseError::unauthorized());
    }
    Ok(())
}

type ApiResult<T> = std::result::Result<T, ApiResponseError>;

struct ApiResponseError {
    status: StatusCode,
    error: anyhow::Error,
}

impl ApiResponseError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            error: anyhow::anyhow!("missing or invalid bearer token"),
        }
    }
}

impl<E> From<E> for ApiResponseError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: error.into(),
        }
    }
}

impl IntoResponse for ApiResponseError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiError {
                error: self.error.to_string(),
            }),
        )
            .into_response()
    }
}

struct DeleteOnDropStream {
    inner: ReaderStream<tokio::fs::File>,
    path: PathBuf,
}

impl futures_util::Stream for DeleteOnDropStream {
    type Item = Result<axum::body::Bytes, io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Drop for DeleteOnDropStream {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
