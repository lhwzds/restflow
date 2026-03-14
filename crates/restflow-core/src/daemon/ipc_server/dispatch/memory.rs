use super::super::runtime::resolve_agent_id;
use super::super::*;

impl IpcServer {
    pub(super) async fn handle_search_memory(
        core: &Arc<AppCore>,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> IpcResponse {
        let agent_id = match resolve_agent_id(core, agent_id) {
            Ok(agent_id) => agent_id,
            Err(err) => return IpcResponse::error(400, err.to_string()),
        };
        let mut search = MemorySearchQuery::new(agent_id);
        if !query.is_empty() {
            search = search.with_query(query);
        }
        if let Some(limit) = limit {
            search = search.paginate(limit, 0);
        }
        match core.storage.memory.search(&search) {
            Ok(result) => IpcResponse::success(result),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_search_memory_ranked(
        core: &Arc<AppCore>,
        query: crate::models::MemorySearchQuery,
        min_score: Option<f64>,
        scoring_preset: Option<String>,
    ) -> IpcResponse {
        let storage = core.storage.memory.clone();
        let mut builder = SearchEngineBuilder::new(storage);
        builder = match scoring_preset.as_deref() {
            Some("frequency_focused") => builder.frequency_focused(),
            Some("recency_focused") => builder.recency_focused(),
            Some("balanced") => builder.balanced(),
            _ => builder,
        };
        if let Some(min_score) = min_score {
            builder = builder.min_score(min_score);
        }
        let engine = builder.build();
        match engine.search_ranked(&query) {
            Ok(result) => IpcResponse::success(result),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_memory_chunk(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match core.storage.memory.get_chunk(&id) {
            Ok(Some(chunk)) => IpcResponse::success(chunk),
            Ok(None) => IpcResponse::not_found("Memory chunk"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_memory(
        core: &Arc<AppCore>,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> IpcResponse {
        let result = match (agent_id, tag) {
            (Some(agent_id), Some(tag)) => {
                core.storage.memory.list_chunks(&agent_id).map(|chunks| {
                    chunks
                        .into_iter()
                        .filter(|chunk| chunk.tags.iter().any(|t| t == &tag))
                        .collect::<Vec<_>>()
                })
            }
            (Some(agent_id), None) => core.storage.memory.list_chunks(&agent_id),
            (None, Some(tag)) => core.storage.memory.list_chunks_by_tag(&tag),
            (None, None) => return IpcResponse::error(400, "agent_id or tag is required"),
        };
        match result {
            Ok(chunks) => IpcResponse::success(chunks),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_memory_by_session(
        core: &Arc<AppCore>,
        session_id: String,
    ) -> IpcResponse {
        match core.storage.memory.list_chunks_for_session(&session_id) {
            Ok(chunks) => IpcResponse::success(chunks),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_add_memory(
        core: &Arc<AppCore>,
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
    ) -> IpcResponse {
        let agent_id = match resolve_agent_id(core, agent_id) {
            Ok(agent_id) => agent_id,
            Err(err) => return IpcResponse::error(400, err.to_string()),
        };
        let mut chunk = MemoryChunk::new(agent_id, content);
        if !tags.is_empty() {
            chunk = chunk.with_tags(tags);
        }
        match core.storage.memory.store_chunk(&chunk) {
            Ok(id) => IpcResponse::success(serde_json::json!({ "id": id })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_memory_chunk(
        core: &Arc<AppCore>,
        chunk: MemoryChunk,
    ) -> IpcResponse {
        match core.storage.memory.store_chunk(&chunk) {
            Ok(id) => {
                if id != chunk.id {
                    match core.storage.memory.get_chunk(&id) {
                        Ok(Some(existing)) => IpcResponse::success(existing),
                        Ok(None) => IpcResponse::error(500, "Stored chunk not found"),
                        Err(err) => IpcResponse::error(500, err.to_string()),
                    }
                } else {
                    IpcResponse::success(chunk)
                }
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_memory(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match core.storage.memory.delete_chunk(&id) {
            Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_clear_memory(
        core: &Arc<AppCore>,
        agent_id: Option<String>,
    ) -> IpcResponse {
        let agent_id = match resolve_agent_id(core, agent_id) {
            Ok(agent_id) => agent_id,
            Err(err) => return IpcResponse::error(400, err.to_string()),
        };
        match core.storage.memory.delete_chunks_for_agent(&agent_id) {
            Ok(count) => IpcResponse::success(serde_json::json!({ "deleted": count })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_memory_stats(
        core: &Arc<AppCore>,
        agent_id: Option<String>,
    ) -> IpcResponse {
        let agent_id = match resolve_agent_id(core, agent_id) {
            Ok(agent_id) => agent_id,
            Err(err) => return IpcResponse::error(400, err.to_string()),
        };
        match core.storage.memory.get_stats(&agent_id) {
            Ok(stats) => IpcResponse::success(stats),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_export_memory(
        core: &Arc<AppCore>,
        agent_id: Option<String>,
    ) -> IpcResponse {
        let agent_id = match resolve_agent_id(core, agent_id) {
            Ok(agent_id) => agent_id,
            Err(err) => return IpcResponse::error(400, err.to_string()),
        };
        let exporter = MemoryExporter::new(core.storage.memory.clone());
        match exporter.export_agent(&agent_id) {
            Ok(result) => IpcResponse::success(result),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_export_memory_session(
        core: &Arc<AppCore>,
        session_id: String,
    ) -> IpcResponse {
        let exporter = MemoryExporter::new(core.storage.memory.clone());
        match exporter.export_session(&session_id) {
            Ok(result) => IpcResponse::success(result),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_export_memory_advanced(
        core: &Arc<AppCore>,
        agent_id: String,
        session_id: Option<String>,
        preset: Option<String>,
        include_metadata: Option<bool>,
        include_timestamps: Option<bool>,
        include_source: Option<bool>,
        include_tags: Option<bool>,
    ) -> IpcResponse {
        let storage = core.storage.memory.clone();
        let mut builder = MemoryExporterBuilder::new(storage);

        builder = match preset.as_deref() {
            Some("minimal") => builder.minimal(),
            Some("compact") => builder.compact(),
            _ => builder,
        };

        if let Some(v) = include_metadata {
            builder = builder.include_metadata(v);
        }
        if let Some(v) = include_timestamps {
            builder = builder.include_timestamps(v);
        }
        if let Some(v) = include_source {
            builder = builder.include_source(v);
        }
        if let Some(v) = include_tags {
            builder = builder.include_tags(v);
        }

        let exporter = builder.build();
        let result = if let Some(session_id) = session_id {
            exporter.export_session(&session_id)
        } else {
            exporter.export_agent(&agent_id)
        };
        match result {
            Ok(result) => IpcResponse::success(result),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_memory_session(
        core: &Arc<AppCore>,
        session_id: String,
    ) -> IpcResponse {
        match core.storage.memory.get_session(&session_id) {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("Memory session"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_memory_sessions(
        core: &Arc<AppCore>,
        agent_id: String,
    ) -> IpcResponse {
        match core.storage.memory.list_sessions(&agent_id) {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_memory_session(
        core: &Arc<AppCore>,
        session: crate::models::MemorySession,
    ) -> IpcResponse {
        match core.storage.memory.create_session(&session) {
            Ok(_) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_memory_session(
        core: &Arc<AppCore>,
        session_id: String,
        delete_chunks: bool,
    ) -> IpcResponse {
        match core
            .storage
            .memory
            .delete_session(&session_id, delete_chunks)
        {
            Ok(deleted) => IpcResponse::success(serde_json::json!({ "deleted": deleted })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
