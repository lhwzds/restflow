use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteWithIdResponse {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveResponse {
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClearResponse {
    pub deleted: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelResponse {
    pub canceled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SteerResponse {
    pub steered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalHandledResponse {
    pub handled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeyResponse {
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptResponse {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretResponse {
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedPeerResponse {
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub approved_at: i64,
    pub approved_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingRequestResponse {
    pub code: String,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub chat_id: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingStateResponse {
    pub allowed_peers: Vec<AllowedPeerResponse>,
    pub pending_requests: Vec<PairingRequestResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingApprovalResponse {
    pub approved: bool,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub owner_chat_id: Option<String>,
    pub owner_auto_bound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingOwnerResponse {
    pub owner_chat_id: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteBindingResponse {
    pub id: String,
    pub binding_type: String,
    pub target_id: String,
    pub agent_id: String,
    pub created_at: i64,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupReportResponse {
    pub chat_sessions: usize,
    pub background_tasks: usize,
    pub checkpoints: usize,
    pub memory_chunks: usize,
    pub memory_sessions: usize,
    pub vector_orphans: usize,
    pub daemon_log_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSourceMigrationResponse {
    pub dry_run: bool,
    pub scanned: usize,
    pub migrated: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcDaemonStatus {
    pub status: String,
    pub protocol_version: String,
    pub daemon_version: String,
    pub pid: u32,
    pub started_at_ms: i64,
    pub uptime_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_roundtrip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let decoded: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, value);
    }

    #[test]
    fn id_response_round_trips() {
        let response = IdResponse {
            id: "memory-1".to_string(),
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn delete_response_round_trips() {
        let response = DeleteResponse { deleted: true };
        assert_roundtrip(&response);
    }

    #[test]
    fn delete_with_id_response_round_trips() {
        let response = DeleteWithIdResponse {
            id: "task-1".to_string(),
            deleted: true,
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn archive_response_round_trips() {
        let response = ArchiveResponse { archived: true };
        assert_roundtrip(&response);
    }

    #[test]
    fn clear_response_round_trips() {
        let response = ClearResponse { deleted: 3 };
        assert_roundtrip(&response);
    }

    #[test]
    fn cancel_response_round_trips() {
        let response = CancelResponse { canceled: true };
        assert_roundtrip(&response);
    }

    #[test]
    fn steer_response_round_trips() {
        let response = SteerResponse { steered: true };
        assert_roundtrip(&response);
    }

    #[test]
    fn approval_handled_response_round_trips() {
        let response = ApprovalHandledResponse { handled: false };
        assert_roundtrip(&response);
    }

    #[test]
    fn ok_response_round_trips() {
        let response = OkResponse { ok: true };
        assert_roundtrip(&response);
    }

    #[test]
    fn secret_response_round_trips() {
        let response = SecretResponse {
            value: Some("token".to_string()),
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn api_key_response_round_trips_with_profile_id() {
        let response = ApiKeyResponse {
            api_key: "key".to_string(),
            profile_id: Some("profile-1".to_string()),
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn prompt_response_round_trips() {
        let response = PromptResponse {
            prompt: "hello".to_string(),
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn daemon_status_round_trips() {
        let response = IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: "2".to_string(),
            daemon_version: "0.4.0".to_string(),
            pid: 42,
            started_at_ms: 123,
            uptime_secs: 456,
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn pairing_state_response_round_trips() {
        let response = PairingStateResponse {
            allowed_peers: vec![AllowedPeerResponse {
                peer_id: "peer-1".to_string(),
                peer_name: Some("Alice".to_string()),
                approved_at: 1,
                approved_by: "cli".to_string(),
            }],
            pending_requests: vec![PairingRequestResponse {
                code: "ABCD1234".to_string(),
                peer_id: "peer-2".to_string(),
                peer_name: None,
                chat_id: "chat-1".to_string(),
                created_at: 2,
                expires_at: 3,
            }],
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn route_binding_response_round_trips() {
        let response = RouteBindingResponse {
            id: "route-1".to_string(),
            binding_type: "channel".to_string(),
            target_id: "telegram".to_string(),
            agent_id: "agent-1".to_string(),
            created_at: 10,
            priority: 2,
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn cleanup_report_response_round_trips() {
        let response = CleanupReportResponse {
            chat_sessions: 1,
            background_tasks: 2,
            checkpoints: 3,
            memory_chunks: 4,
            memory_sessions: 5,
            vector_orphans: 6,
            daemon_log_files: 7,
        };
        assert_roundtrip(&response);
    }

    #[test]
    fn session_source_migration_response_round_trips() {
        let response = SessionSourceMigrationResponse {
            dry_run: true,
            scanned: 10,
            migrated: 4,
            skipped: 5,
            failed: 1,
        };
        assert_roundtrip(&response);
    }
}
