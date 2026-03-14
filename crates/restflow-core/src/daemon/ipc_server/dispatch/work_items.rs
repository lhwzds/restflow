use super::super::*;

impl IpcServer {
    pub(super) async fn handle_list_work_items(
        core: &Arc<AppCore>,
        query: crate::models::ItemQuery,
    ) -> IpcResponse {
        match core.storage.work_items.list_notes(query) {
            Ok(items) => IpcResponse::success(items),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_work_item_folders(core: &Arc<AppCore>) -> IpcResponse {
        match core.storage.work_items.list_folders() {
            Ok(folders) => IpcResponse::success(folders),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_work_item(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match core.storage.work_items.get_note(&id) {
            Ok(Some(item)) => IpcResponse::success(item),
            Ok(None) => IpcResponse::not_found("Work item"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_work_item(
        core: &Arc<AppCore>,
        spec: crate::models::WorkItemSpec,
    ) -> IpcResponse {
        match core.storage.work_items.create_note(spec) {
            Ok(item) => IpcResponse::success(item),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_work_item(
        core: &Arc<AppCore>,
        id: String,
        patch: crate::models::WorkItemPatch,
    ) -> IpcResponse {
        match core.storage.work_items.update_note(&id, patch) {
            Ok(item) => IpcResponse::success(item),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_work_item(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match core.storage.work_items.delete_note(&id) {
            Ok(()) => IpcResponse::success(serde_json::json!({ "ok": true })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
