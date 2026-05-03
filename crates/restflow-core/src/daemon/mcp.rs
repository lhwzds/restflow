use crate::AppCore;
use crate::daemon::{IpcRequest, IpcResponse, IpcServer, StreamFrame};
use crate::mcp::RestFlowMcpServer;
use crate::models::storage_mode::StorageMode;
use crate::models::{GatingCheckResult, Skill, SkillManifest, SkillVersion, TaskConversionResult};
use crate::registry::{
    GatingChecker, GitHubProvider, MarketplaceProvider, SkillProvider as _, SkillSearchQuery,
    SkillSearchResult, SkillSortOrder,
};
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::services::task_command::{TaskCommandService, TaskExecutionMode};
use anyhow::Result;
use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, post_service};
use bytes::Bytes;
use futures::StreamExt;
use http::{
    Response as HttpResponse,
    header::{CONTENT_LENGTH, HeaderName},
};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use restflow_contracts::ErrorPayload;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower::util::MapResponseLayer;
use tracing::info;

use super::ipc_protocol::IpcDaemonStatus;

const ERROR_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const RECOVERY_HEADER: &str = "x-restflow-mcp-recover";
const RECOVERY_REINITIALIZE: &str = "reinitialize";
const NDJSON_CONTENT_TYPE: &str = "application/x-ndjson; charset=utf-8";
type McpHttpBody = BoxBody<Bytes, Infallible>;
type McpHttpResponse = HttpResponse<McpHttpBody>;

type RuntimeToolRegistry = restflow_ai::tools::ToolRegistry;

#[derive(Clone)]
struct DaemonHttpState {
    core: Arc<AppCore>,
    runtime_tool_registry: Arc<OnceLock<RuntimeToolRegistry>>,
}

#[derive(Debug, Deserialize)]
struct MarketplaceSearchRequest {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    include_github: Option<bool>,
}

#[derive(Debug, Serialize)]
struct MarketplaceSearchItem {
    manifest: SkillManifest,
    score: u32,
    downloads: Option<u64>,
    rating: Option<f32>,
    source: String,
}

#[derive(Debug, Deserialize)]
struct MarketplaceGetRequest {
    id: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MarketplaceContentRequest {
    id: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MarketplaceInstallRequest {
    id: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

pub async fn run_mcp_http_server(
    core: Arc<AppCore>,
    addr: SocketAddr,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let cancellation = CancellationToken::new();
    let app = build_http_router(core, cancellation.clone());
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "Daemon HTTP server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
            cancellation.cancel();
        })
        .await?;

    Ok(())
}

fn build_http_router(core: Arc<AppCore>, cancellation: CancellationToken) -> Router {
    let config = build_streamable_http_server_config(cancellation);
    let server_factory = build_mcp_server_factory(RestFlowMcpServer::new(core.clone()));

    let mcp_service = StreamableHttpService::new(
        server_factory,
        LocalSessionManager::default().into(),
        config,
    );
    let mcp_service = ServiceBuilder::new()
        .layer(MapResponseLayer::new(normalize_mcp_error_response))
        .service(mcp_service);

    let state = DaemonHttpState {
        core,
        runtime_tool_registry: Arc::new(OnceLock::new()),
    };

    Router::new()
        .route("/api/health", get(api_health))
        .route("/health", get(api_health))
        .route("/api/request", post(api_request))
        .route("/api/stream", post(api_stream))
        .route(
            "/api/tasks/convert-session",
            post(api_convert_session_to_task),
        )
        .route("/api/marketplace/search", post(api_marketplace_search))
        .route("/api/marketplace/skill", post(api_marketplace_get_skill))
        .route(
            "/api/marketplace/versions",
            post(api_marketplace_get_versions),
        )
        .route(
            "/api/marketplace/content",
            post(api_marketplace_get_content),
        )
        .route(
            "/api/marketplace/gating",
            post(api_marketplace_check_gating),
        )
        .route(
            "/api/marketplace/install",
            post(api_marketplace_install_skill),
        )
        .route(
            "/api/marketplace/uninstall",
            post(api_marketplace_uninstall_skill),
        )
        .route(
            "/api/marketplace/installed",
            get(api_marketplace_list_installed),
        )
        .route_service("/mcp", post_service(mcp_service))
        .fallback(get(not_found))
        .with_state(state)
}

async fn api_health() -> Json<IpcDaemonStatus> {
    Json(super::ipc_server::build_daemon_status())
}

async fn api_request(
    State(state): State<DaemonHttpState>,
    Json(request): Json<IpcRequest>,
) -> Json<IpcResponse> {
    let response =
        IpcServer::process(&state.core, state.runtime_tool_registry.as_ref(), request).await;
    Json(response)
}

async fn api_stream(
    State(state): State<DaemonHttpState>,
    Json(request): Json<IpcRequest>,
) -> Response {
    let receiver = match IpcServer::open_stream(state.core.clone(), request).await {
        Ok(receiver) => receiver,
        Err(error) => single_frame_channel(StreamFrame::error(400, error.to_string())),
    };

    stream_frames_response(receiver)
}

fn provider_name(source: Option<&str>) -> &str {
    match source {
        Some("github") => "github",
        _ => "marketplace",
    }
}

fn is_reserved_systemskill_id(id: &str) -> bool {
    crate::skill_files::systemskill_ids().any(|system_id| system_id == id)
}

fn is_marketplace_source_ref(source_ref: &str) -> bool {
    source_ref.starts_with("marketplace:")
        || source_ref.starts_with("github:")
        || source_ref.starts_with("mcp_marketplace:")
        || source_ref.starts_with("mcp_github:")
}

fn is_marketplace_installed_skill(skill: &Skill) -> bool {
    skill.source == crate::models::SkillSource::External
        && skill
            .source_ref
            .as_deref()
            .is_some_and(is_marketplace_source_ref)
}

fn search_sort_order(sort: Option<String>) -> Option<SkillSortOrder> {
    match sort.as_deref() {
        Some("relevance") => Some(SkillSortOrder::Relevance),
        Some("updated") | Some("recently_updated") => Some(SkillSortOrder::RecentlyUpdated),
        Some("popular") | Some("downloads") => Some(SkillSortOrder::Popular),
        Some("name") => Some(SkillSortOrder::Name),
        _ => None,
    }
}

fn to_marketplace_search_item(result: SkillSearchResult, source: &str) -> MarketplaceSearchItem {
    MarketplaceSearchItem {
        manifest: result.manifest,
        score: result.score,
        downloads: result.downloads,
        rating: result.rating,
        source: source.to_string(),
    }
}

fn resolve_content_version(
    requested: Option<String>,
    fallback: Option<SkillVersion>,
) -> std::result::Result<SkillVersion, String> {
    if let Some(version) = requested {
        SkillVersion::parse(&version).ok_or_else(|| format!("Invalid version: {version}"))
    } else {
        fallback.ok_or_else(|| "Version is required".to_string())
    }
}

fn manifest_to_skill(source_name: &str, manifest: SkillManifest, content: String) -> Skill {
    let gating = if manifest.gating.binaries.is_empty()
        && manifest.gating.env_vars.is_empty()
        && manifest.gating.supported_os.is_empty()
    {
        None
    } else {
        Some(crate::models::SkillGating {
            bins: if manifest.gating.binaries.is_empty() {
                None
            } else {
                Some(
                    manifest
                        .gating
                        .binaries
                        .iter()
                        .map(|binary| binary.name.clone())
                        .collect(),
                )
            },
            env: if manifest.gating.env_vars.is_empty() {
                None
            } else {
                Some(
                    manifest
                        .gating
                        .env_vars
                        .iter()
                        .map(|env_var| env_var.name.clone())
                        .collect(),
                )
            },
            os: if manifest.gating.supported_os.is_empty() {
                None
            } else {
                Some(
                    manifest
                        .gating
                        .supported_os
                        .iter()
                        .map(|os| match os {
                            crate::models::OsType::Windows => "windows".to_string(),
                            crate::models::OsType::MacOS => "macos".to_string(),
                            crate::models::OsType::Linux => "linux".to_string(),
                            crate::models::OsType::Any => "any".to_string(),
                        })
                        .collect(),
                )
            },
        })
    };

    Skill {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        tags: Some(manifest.keywords.clone()),
        triggers: Vec::new(),
        content,
        folder_path: None,
        suggested_tools: Vec::new(),
        scripts: Vec::new(),
        references: Vec::new(),
        gating,
        version: Some(manifest.version.to_string()),
        author: manifest.author.as_ref().map(|author| author.name.clone()),
        license: manifest.license.clone(),
        content_hash: None,
        status: crate::models::SkillStatus::Active,
        auto_complete: false,
        storage_mode: StorageMode::DatabaseOnly,
        is_synced: false,
        source: crate::models::SkillSource::External,
        read_only: false,
        source_ref: Some(format!(
            "mcp_{source_name}:{}@{}",
            manifest.id, manifest.version
        )),
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    }
}

async fn api_convert_session_to_task(
    State(state): State<DaemonHttpState>,
    Json(request): Json<restflow_contracts::request::TaskFromSessionRequest>,
) -> std::result::Result<Json<TaskConversionResult>, (StatusCode, Json<ErrorPayload>)> {
    let store_request =
        crate::boundary::task::contract_convert_request_to_store(request).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorPayload::new(400, error.to_string(), None)),
            )
        })?;

    let outcome = TaskCommandService::from_storage(
        state.core.storage.as_ref(),
        Some(Arc::new(OperationAssessorAdapter::new(state.core.clone()))),
    )
    .convert_session(store_request, TaskExecutionMode::Direct)
    .await
    .and_then(TaskCommandService::into_direct_result)
    .map_err(|error| {
        let status =
            StatusCode::from_u16(error.code() as u16).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(error.payload()))
    })?;

    Ok(Json(outcome))
}

async fn api_marketplace_search(
    Json(request): Json<MarketplaceSearchRequest>,
) -> std::result::Result<Json<Vec<MarketplaceSearchItem>>, (StatusCode, String)> {
    let query = SkillSearchQuery {
        query: request.query,
        category: request.category,
        tags: request.tags.unwrap_or_default(),
        author: request.author,
        limit: request.limit,
        offset: request.offset,
        sort: search_sort_order(request.sort),
    };

    let mut results = MarketplaceProvider::new()
        .search(&query)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?
        .into_iter()
        .map(|result| to_marketplace_search_item(result, "marketplace"))
        .collect::<Vec<_>>();

    if request.include_github.unwrap_or(false) {
        let github_results = GitHubProvider::new()
            .search(&query)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        results.extend(
            github_results
                .into_iter()
                .map(|result| to_marketplace_search_item(result, "github")),
        );
        results.sort_by_key(|result| std::cmp::Reverse(result.score));
        if let Some(limit) = request.limit {
            results.truncate(limit);
        }
    }

    Ok(Json(results))
}

async fn api_marketplace_get_skill(
    Json(request): Json<MarketplaceGetRequest>,
) -> std::result::Result<Json<SkillManifest>, (StatusCode, String)> {
    let manifest = match provider_name(request.source.as_deref()) {
        "github" => GitHubProvider::new()
            .get_manifest(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
        _ => MarketplaceProvider::new()
            .get_manifest(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
    };

    Ok(Json(manifest))
}

async fn api_marketplace_get_versions(
    Json(request): Json<MarketplaceGetRequest>,
) -> std::result::Result<Json<Vec<SkillVersion>>, (StatusCode, String)> {
    let versions = match provider_name(request.source.as_deref()) {
        "github" => GitHubProvider::new()
            .list_versions(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
        _ => MarketplaceProvider::new()
            .list_versions(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
    };

    Ok(Json(versions))
}

async fn api_marketplace_get_content(
    Json(request): Json<MarketplaceContentRequest>,
) -> std::result::Result<Json<String>, (StatusCode, String)> {
    let content = match provider_name(request.source.as_deref()) {
        "github" => {
            let provider = GitHubProvider::new();
            let fallback_version = if request.version.is_none() {
                Some(
                    provider
                        .get_manifest(&request.id)
                        .await
                        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?
                        .version,
                )
            } else {
                None
            };
            let version = resolve_content_version(request.version, fallback_version)
                .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
            provider
                .get_content(&request.id, &version)
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?
        }
        _ => {
            let provider = MarketplaceProvider::new();
            let fallback_version = if request.version.is_none() {
                Some(
                    provider
                        .get_manifest(&request.id)
                        .await
                        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?
                        .version,
                )
            } else {
                None
            };
            let version = resolve_content_version(request.version, fallback_version)
                .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
            provider
                .get_content(&request.id, &version)
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?
        }
    };

    Ok(Json(content))
}

async fn api_marketplace_check_gating(
    Json(request): Json<MarketplaceGetRequest>,
) -> std::result::Result<Json<GatingCheckResult>, (StatusCode, String)> {
    let manifest = match provider_name(request.source.as_deref()) {
        "github" => GitHubProvider::new()
            .get_manifest(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
        _ => MarketplaceProvider::new()
            .get_manifest(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
    };

    Ok(Json(GatingChecker::default().check(&manifest.gating)))
}

async fn api_marketplace_install_skill(
    State(state): State<DaemonHttpState>,
    Json(request): Json<MarketplaceInstallRequest>,
) -> std::result::Result<StatusCode, (StatusCode, String)> {
    if is_reserved_systemskill_id(&request.id) {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot install marketplace skill over systemskill: {}",
                request.id
            ),
        ));
    }

    let source_name = provider_name(request.source.as_deref());
    let manifest = match source_name {
        "github" => GitHubProvider::new()
            .get_manifest(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
        _ => MarketplaceProvider::new()
            .get_manifest(&request.id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
    };
    if is_reserved_systemskill_id(&manifest.id) {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot install marketplace skill over systemskill: {}",
                manifest.id
            ),
        ));
    }

    let gating_result = GatingChecker::default().check(&manifest.gating);
    if !gating_result.passed {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Gating requirements not met: {}", gating_result.summary),
        ));
    }

    let version = request
        .version
        .and_then(|value| SkillVersion::parse(&value));
    let content_version = version.unwrap_or_else(|| manifest.version.clone());
    let content = match source_name {
        "github" => GitHubProvider::new()
            .get_content(&request.id, &content_version)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
        _ => MarketplaceProvider::new()
            .get_content(&request.id, &content_version)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?,
    };

    let skill = manifest_to_skill(source_name, manifest, content);
    if state
        .core
        .storage
        .skills
        .exists(&skill.id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    {
        state
            .core
            .storage
            .skills
            .update(&skill.id, &skill)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    } else {
        state
            .core
            .storage
            .skills
            .create(&skill)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn api_marketplace_uninstall_skill(
    State(state): State<DaemonHttpState>,
    Json(request): Json<MarketplaceGetRequest>,
) -> std::result::Result<StatusCode, (StatusCode, String)> {
    let skill = state
        .core
        .storage
        .skills
        .get(&request.id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let Some(skill) = skill else {
        return Ok(StatusCode::NO_CONTENT);
    };
    if !is_marketplace_installed_skill(&skill) {
        return Err((
            StatusCode::CONFLICT,
            format!("Skill is not a marketplace installation: {}", request.id),
        ));
    }
    state
        .core
        .storage
        .skills
        .delete(&request.id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn api_marketplace_list_installed(
    State(state): State<DaemonHttpState>,
) -> std::result::Result<Json<Vec<Skill>>, (StatusCode, String)> {
    let skills = state
        .core
        .storage
        .skills
        .list()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(
        skills
            .into_iter()
            .filter(is_marketplace_installed_skill)
            .collect(),
    ))
}

fn stream_frames_response(receiver: mpsc::UnboundedReceiver<StreamFrame>) -> Response {
    let stream = UnboundedReceiverStream::new(receiver).map(|frame| {
        let mut bytes = match serde_json::to_vec(&frame) {
            Ok(bytes) => bytes,
            Err(error) => serde_json::to_vec(&StreamFrame::error(
                500,
                format!("Failed to encode stream frame: {error}"),
            ))
            .expect("stream error frame serialization"),
        };
        bytes.push(b'\n');
        Ok::<Bytes, Infallible>(Bytes::from(bytes))
    });

    (
        [(CONTENT_TYPE, HeaderValue::from_static(NDJSON_CONTENT_TYPE))],
        Body::from_stream(stream),
    )
        .into_response()
}

fn single_frame_channel(frame: StreamFrame) -> mpsc::UnboundedReceiver<StreamFrame> {
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(frame);
    rx
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "not_found",
            "message": "RestFlow exposes daemon HTTP and MCP APIs only.",
        })),
    )
}

fn build_mcp_server_factory(
    server: RestFlowMcpServer,
) -> impl Fn() -> std::result::Result<RestFlowMcpServer, std::io::Error> + Clone {
    move || Ok(server.clone())
}

fn build_streamable_http_server_config(
    cancellation_token: CancellationToken,
) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_cancellation_token(cancellation_token)
}

fn classify_error_status(status: StatusCode) -> Option<(&'static str, &'static str, bool)> {
    match status {
        StatusCode::UNAUTHORIZED => Some((
            "session_expired",
            "MCP session is missing or expired. Reinitialize the session and retry.",
            true,
        )),
        StatusCode::NOT_ACCEPTABLE => Some((
            "invalid_accept_header",
            "The client must accept the required MCP media types.",
            false,
        )),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => Some((
            "invalid_content_type",
            "Content-Type must be application/json for MCP POST requests.",
            false,
        )),
        StatusCode::UNPROCESSABLE_ENTITY => Some((
            "invalid_request",
            "The request payload does not satisfy MCP protocol requirements.",
            false,
        )),
        StatusCode::METHOD_NOT_ALLOWED => Some((
            "method_not_allowed",
            "The HTTP method is not supported for this MCP endpoint.",
            false,
        )),
        _ => None,
    }
}

fn normalize_mcp_error_response(response: McpHttpResponse) -> McpHttpResponse {
    if response.headers().contains_key(CONTENT_TYPE) {
        return response;
    }

    let status = response.status();
    let Some((code, message, recoverable)) = classify_error_status(status) else {
        return response;
    };

    let mut builder = HttpResponse::builder().status(status);
    for (name, value) in response.headers() {
        if name == CONTENT_TYPE || name == CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(name, value.clone());
    }
    builder = builder.header(CONTENT_TYPE, HeaderValue::from_static(ERROR_CONTENT_TYPE));
    if recoverable {
        builder = builder.header(
            HeaderName::from_static(RECOVERY_HEADER),
            HeaderValue::from_static(RECOVERY_REINITIALIZE),
        );
    }

    let payload = json!({
        "error": {
            "code": code,
            "message": message,
            "recoverable": recoverable
        }
    });

    builder
        .body(Full::new(Bytes::from(payload.to_string())).boxed())
        .expect("valid MCP error response")
}

#[cfg(test)]
fn is_expected_connection_close(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    [
        "connection closed before message completed",
        "connection error",
        "connection reset by peer",
        "broken pipe",
        "operation canceled",
        "cancelled",
        "closed",
        "eof",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::{
        ERROR_CONTENT_TYPE, NDJSON_CONTENT_TYPE, RECOVERY_HEADER, RECOVERY_REINITIALIZE,
        build_http_router, build_mcp_server_factory, build_streamable_http_server_config,
        is_expected_connection_close, is_marketplace_installed_skill, is_marketplace_source_ref,
        is_reserved_systemskill_id, manifest_to_skill, normalize_mcp_error_response,
    };
    use crate::AppCore;
    use crate::daemon::session_events::ChatSessionEvent;
    use crate::daemon::{
        IpcRequest, IpcResponse, IpcStreamEvent, StreamFrame, publish_session_event,
    };
    use crate::models::{
        AgentNode, ChatMessage, ChatSession, ModelId, Skill, SkillManifest, SkillSource,
        SkillVersion,
    };
    use axum::body::{self, Body};
    use axum::http::{HeaderValue, Request, StatusCode, header::CONTENT_TYPE};
    use bytes::Bytes;
    use futures::StreamExt;
    use http::Response;
    use http_body_util::{BodyExt, Full};
    use serde_json::Value;
    use std::env;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::tempdir;
    use tokio::time::Duration;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct EnvGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set_path(key: &'static str, path: &Path) -> Self {
            let original = env::var_os(key);
            unsafe {
                env::set_var(key, path);
            }
            Self { key, original }
        }

        fn set_value(key: &'static str, value: &str) -> Self {
            let original = env::var_os(key);
            unsafe {
                env::set_var(key, value);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                unsafe {
                    env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    #[allow(clippy::await_holding_lock)]
    async fn test_core() -> Arc<AppCore> {
        let _restflow_dir_lock = crate::paths::restflow_dir_env_lock();
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("daemon-http-test.db");
        let state_dir = temp.path().join("state");
        std::fs::create_dir_all(&state_dir).expect("state dir");
        let _restflow_dir = EnvGuard::set_path("RESTFLOW_DIR", &state_dir);
        let _master_key = EnvGuard::set_value("RESTFLOW_MASTER_KEY", &"11".repeat(32));
        Arc::new(
            AppCore::new(db_path.to_str().expect("db path utf8"))
                .await
                .expect("app core"),
        )
    }

    #[test]
    fn classify_expected_connection_shutdown_errors() {
        assert!(is_expected_connection_close(
            "connection closed before message completed"
        ));
        assert!(is_expected_connection_close("connection error"));
        assert!(is_expected_connection_close("broken pipe"));
    }

    #[test]
    fn classify_unexpected_connection_errors() {
        assert!(!is_expected_connection_close("tls handshake failed"));
        assert!(!is_expected_connection_close("http parse failure"));
    }

    #[test]
    fn mcp_http_server_config_uses_stateless_mode() {
        let config = build_streamable_http_server_config(CancellationToken::new());
        assert!(!config.stateful_mode);
    }

    #[test]
    fn marketplace_manifest_to_skill_preserves_provider_source_ref() {
        let manifest = SkillManifest {
            id: "demo-skill".to_string(),
            name: "Demo Skill".to_string(),
            version: SkillVersion::new(1, 2, 3),
            ..SkillManifest::default()
        };

        let skill = manifest_to_skill("github", manifest, "# Demo".to_string());

        assert_eq!(
            skill.source_ref.as_deref(),
            Some("mcp_github:demo-skill@1.2.3")
        );
    }

    #[test]
    fn marketplace_install_rejects_systemskill_ids() {
        assert!(is_reserved_systemskill_id("team"));
        assert!(!is_reserved_systemskill_id("demo-skill"));
    }

    #[test]
    fn marketplace_installed_filter_requires_marketplace_source_ref() {
        assert!(is_marketplace_source_ref("marketplace:demo@1.0.0"));
        assert!(is_marketplace_source_ref("github:demo@1.0.0"));
        assert!(is_marketplace_source_ref("mcp_marketplace:demo@1.0.0"));
        assert!(is_marketplace_source_ref("mcp_github:demo@1.0.0"));
        assert!(!is_marketplace_source_ref("package:/tmp/demo.skill"));

        let mut skill = Skill::new(
            "demo".to_string(),
            "Demo".to_string(),
            None,
            None,
            "# Demo".to_string(),
        );
        assert!(!is_marketplace_installed_skill(&skill));

        skill.source = SkillSource::External;
        skill.source_ref = Some("mcp_marketplace:demo@1.0.0".to_string());
        assert!(is_marketplace_installed_skill(&skill));

        skill.source_ref = Some("package:/tmp/demo.skill".to_string());
        assert!(!is_marketplace_installed_skill(&skill));
    }

    #[tokio::test]
    async fn api_marketplace_installed_returns_only_marketplace_sources() {
        let core = test_core().await;
        let mut marketplace_skill = Skill::new(
            "marketplace-skill".to_string(),
            "Marketplace Skill".to_string(),
            None,
            None,
            "# Marketplace".to_string(),
        );
        marketplace_skill.source = SkillSource::External;
        marketplace_skill.source_ref = Some("mcp_marketplace:marketplace-skill@1.0.0".to_string());
        core.storage
            .skills
            .create(&marketplace_skill)
            .expect("create marketplace skill");

        let user_skill = Skill::new(
            "user-skill".to_string(),
            "User Skill".to_string(),
            None,
            None,
            "# User".to_string(),
        );
        core.storage
            .skills
            .create(&user_skill)
            .expect("create user skill");

        let app = build_http_router(core, CancellationToken::new());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/marketplace/installed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let skills: Vec<Skill> = serde_json::from_slice(&body).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "marketplace-skill");
    }

    #[tokio::test]
    async fn api_marketplace_uninstall_rejects_user_skill() {
        let core = test_core().await;
        let user_skill = Skill::new(
            "user-skill".to_string(),
            "User Skill".to_string(),
            None,
            None,
            "# User".to_string(),
        );
        core.storage
            .skills
            .create(&user_skill)
            .expect("create user skill");

        let app = build_http_router(core.clone(), CancellationToken::new());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/marketplace/uninstall")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({ "id": "user-skill" })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(
            core.storage
                .skills
                .exists("user-skill")
                .expect("skill exists check")
        );
    }

    #[tokio::test]
    async fn mcp_server_factory_returns_cloneable_server_instances() {
        let core = test_core().await;
        let factory = build_mcp_server_factory(super::RestFlowMcpServer::new(core));

        let first = factory().expect("first server");
        let second = factory().expect("second server");
        drop((first, second));
    }

    #[tokio::test]
    async fn normalize_mcp_error_response_adds_json_error_payload() {
        let response = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Full::new(Bytes::from_static(b"Unauthorized: Session not found")).boxed())
            .unwrap();

        let normalized = normalize_mcp_error_response(response);

        assert_eq!(
            normalized.headers().get(CONTENT_TYPE).unwrap(),
            &HeaderValue::from_static(ERROR_CONTENT_TYPE)
        );
        assert_eq!(
            normalized.headers().get(RECOVERY_HEADER).unwrap(),
            &HeaderValue::from_static(RECOVERY_REINITIALIZE)
        );

        let body = normalized.into_body().collect().await.unwrap().to_bytes();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "session_expired");
        assert_eq!(payload["error"]["recoverable"], true);
    }

    #[tokio::test]
    async fn normalize_mcp_error_response_keeps_existing_content_type() {
        let response = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(CONTENT_TYPE, "text/plain")
            .body(Full::new(Bytes::from_static(b"already tagged")).boxed())
            .unwrap();

        let normalized = normalize_mcp_error_response(response);
        assert_eq!(
            normalized.headers().get(CONTENT_TYPE).unwrap(),
            "text/plain"
        );

        let body = normalized.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"already tagged");
    }

    #[tokio::test]
    async fn api_health_returns_daemon_status() {
        let app = build_http_router(test_core().await, CancellationToken::new());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["status"], "running");
        assert_eq!(payload["protocol_version"], "2");
    }

    #[tokio::test]
    async fn api_request_round_trips_ipc_request() {
        let app = build_http_router(test_core().await, CancellationToken::new());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/request")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&IpcRequest::GetStatus).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: IpcResponse = serde_json::from_slice(&body).unwrap();
        match payload {
            IpcResponse::Success(value) => assert_eq!(value["status"], "running"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn api_convert_session_returns_direct_conversion_result() {
        let _env_lock = env_lock();
        let core = test_core().await;
        let agents_dir = tempdir().expect("agents tempdir");
        let _agents_dir =
            EnvGuard::set_path(crate::prompt_files::AGENTS_DIR_ENV, agents_dir.path());
        std::fs::create_dir_all(agents_dir.path()).expect("create agents dir");

        let agent = core
            .storage
            .agents
            .create_agent("http-bg-owner".to_string(), AgentNode::new())
            .expect("create agent");
        let mut session = ChatSession::new(
            agent.id.clone(),
            ModelId::Gpt5_4.as_serialized_str().to_string(),
        )
        .with_name("HTTP Convert Session");
        session.add_message(ChatMessage::user("Continue this job in background"));
        core.storage
            .chat_sessions
            .create(&session)
            .expect("create session");

        let app = build_http_router(core, CancellationToken::new());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tasks/convert-session")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "session_id": session.id,
                            "name": "HTTP Converted Task",
                            "run_now": false
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: crate::models::TaskConversionResult =
            serde_json::from_slice(&body).expect("conversion outcome");
        assert_eq!(result.source_session_id, session.id);
        assert_eq!(result.task.chat_session_id, session.id);
        assert_eq!(result.task.name, "HTTP Converted Task");
        assert!(!result.run_now);
    }

    #[tokio::test]
    async fn api_stream_emits_ndjson_frames() {
        let app = build_http_router(test_core().await, CancellationToken::new());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/stream")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&IpcRequest::SubscribeSessionEvents).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            NDJSON_CONTENT_TYPE
        );

        publish_session_event(ChatSessionEvent::Updated {
            session_id: "session-1".to_string(),
        });

        let mut body_stream = response.into_body().into_data_stream();
        let mut buffer = Vec::new();
        let mut lines = Vec::new();

        while lines.len() < 2 {
            let chunk = tokio::time::timeout(Duration::from_secs(1), body_stream.next())
                .await
                .expect("stream frame timeout")
                .expect("stream frame")
                .expect("stream bytes");
            buffer.extend_from_slice(&chunk);
            lines = buffer
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| line.to_vec())
                .collect();
        }

        let first: StreamFrame = serde_json::from_slice(&lines[0]).unwrap();
        let second: StreamFrame = serde_json::from_slice(&lines[1]).unwrap();

        assert!(matches!(first, StreamFrame::Start { .. }));
        assert!(matches!(
            second,
            StreamFrame::Event {
                event: IpcStreamEvent::Session(_)
            }
        ));
    }

    #[tokio::test]
    async fn unknown_routes_return_api_not_found() {
        let app = build_http_router(test_core().await, CancellationToken::new());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/workspace")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
