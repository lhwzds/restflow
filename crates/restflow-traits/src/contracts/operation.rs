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
pub struct CleanupReportResponse {
    pub chat_sessions: usize,
    pub tasks: usize,
    pub audit_events: usize,
    /// Deprecated compatibility field. Telemetry metric samples are no longer a
    /// separate projection table; this remains zero for one release cycle.
    #[serde(default)]
    pub telemetry_metric_samples: usize,
    pub daemon_log_files: usize,
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
    fn cleanup_report_response_round_trips() {
        let response = CleanupReportResponse {
            chat_sessions: 1,
            tasks: 2,
            audit_events: 3,
            telemetry_metric_samples: 0,
            daemon_log_files: 4,
        };
        assert_roundtrip(&response);
    }
}
