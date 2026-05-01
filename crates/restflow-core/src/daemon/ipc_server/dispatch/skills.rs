use super::super::*;
use restflow_contracts::OkResponse;

impl IpcServer {
    pub(super) async fn handle_list_skills(core: &Arc<AppCore>) -> IpcResponse {
        match skills_service::list_skills(core).await {
            Ok(skills) => IpcResponse::success(skills),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_skill(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match skills_service::get_skill(core, &id).await {
            Ok(Some(skill)) => IpcResponse::success(skill),
            Ok(None) => IpcResponse::not_found("Skill"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_skill(
        core: &Arc<AppCore>,
        skill: crate::models::Skill,
    ) -> IpcResponse {
        match skills_service::create_skill(core, skill).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_skill(
        core: &Arc<AppCore>,
        id: String,
        skill: crate::models::Skill,
    ) -> IpcResponse {
        match skills_service::update_skill(core, &id, &skill).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_skill_reference(
        core: &Arc<AppCore>,
        skill_id: String,
        ref_id: String,
    ) -> IpcResponse {
        match skills_service::get_skill_reference(core, &skill_id, &ref_id).await {
            Ok(Some(content)) => IpcResponse::success(content),
            Ok(None) => IpcResponse::not_found("Skill reference"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_skill(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match skills_service::delete_skill(core, &id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
