//! Telegram DM Pairing - Access control for channel interactions.
//!
//! Provides a pairing mechanism where unknown Telegram users must present
//! a code that is approved by the admin via CLI before they can interact
//! with the bot.

use anyhow::{Result, anyhow};
use rand::RngExt;
use rand::distr::Alphanumeric;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use restflow_storage::PairingStorage;

/// An allowed peer that has been paired/approved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedPeer {
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub approved_at: i64,
    pub approved_by: String,
}

/// A pending pairing request with a time-limited code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequest {
    pub code: String,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub chat_id: String,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Default pairing code expiry: 1 hour in milliseconds.
const DEFAULT_EXPIRY_MS: i64 = 3_600_000;

/// Manages peer pairing (access control) for channel interactions.
pub struct PairingManager {
    storage: Arc<PairingStorage>,
}

impl PairingManager {
    /// Create a new PairingManager.
    pub fn new(storage: Arc<PairingStorage>) -> Self {
        Self { storage }
    }

    /// Check if a peer is allowed to interact with the bot.
    pub fn is_allowed(&self, peer_id: &str) -> Result<bool> {
        self.storage.is_peer_allowed(peer_id)
    }

    /// Directly allow a peer without pairing-code approval flow.
    /// Intended for bootstrap and admin-controlled setup.
    pub fn allow_peer(
        &self,
        peer_id: &str,
        peer_name: Option<&str>,
        approved_by: &str,
    ) -> Result<AllowedPeer> {
        let peer = AllowedPeer {
            peer_id: peer_id.to_string(),
            peer_name: peer_name.map(|s| s.to_string()),
            approved_at: chrono::Utc::now().timestamp_millis(),
            approved_by: approved_by.to_string(),
        };
        let peer_data = serde_json::to_vec(&peer)?;
        self.storage.add_peer(peer_id, &peer_data)?;
        Ok(peer)
    }

    /// Check if a peer has a pending pairing request.
    pub fn has_pending_request(&self, peer_id: &str) -> Result<bool> {
        let code = self.storage.get_pairing_request_by_peer(peer_id)?;
        if let Some(code) = code {
            // Verify the request still exists and is not expired
            if let Some(data) = self.storage.get_pairing_request(&code)? {
                let req: PairingRequest = serde_json::from_slice(&data)?;
                let now = chrono::Utc::now().timestamp_millis();
                if req.expires_at > now {
                    return Ok(true);
                }
                // Expired, clean up
                let _ = self.storage.delete_pairing_request(&code);
            }
        }
        Ok(false)
    }

    /// Generate a pairing code for an unknown peer.
    /// Returns the 8-char alphanumeric code.
    pub fn create_request(
        &self,
        peer_id: &str,
        peer_name: Option<&str>,
        chat_id: &str,
    ) -> Result<String> {
        if let Some(existing_code) = self.storage.get_pairing_request_by_peer(peer_id)?
            && let Some(data) = self.storage.get_pairing_request(&existing_code)?
        {
            let request: PairingRequest = serde_json::from_slice(&data)?;
            let now = chrono::Utc::now().timestamp_millis();
            if request.expires_at > now {
                return Ok(existing_code);
            }
            // Expired request can be replaced by a new one.
            let _ = self.storage.delete_pairing_request(&existing_code);
        }

        let code: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        let now = chrono::Utc::now().timestamp_millis();
        let request = PairingRequest {
            code: code.clone(),
            peer_id: peer_id.to_string(),
            peer_name: peer_name.map(|s| s.to_string()),
            chat_id: chat_id.to_string(),
            created_at: now,
            expires_at: now + DEFAULT_EXPIRY_MS,
        };

        let data = serde_json::to_vec(&request)?;
        self.storage.create_pairing_request(&code, peer_id, &data)?;

        Ok(code)
    }

    /// Approve a pairing request by code. Returns the approved peer.
    pub fn approve(&self, code: &str, approved_by: &str) -> Result<AllowedPeer> {
        let (peer, _) = self.approve_with_request(code, approved_by)?;
        Ok(peer)
    }

    /// Approve a pairing request and return both approved peer and source request.
    pub fn approve_with_request(
        &self,
        code: &str,
        approved_by: &str,
    ) -> Result<(AllowedPeer, PairingRequest)> {
        let data = self
            .storage
            .get_pairing_request(code)?
            .ok_or_else(|| anyhow!("Pairing request not found: {}", code))?;

        let request: PairingRequest = serde_json::from_slice(&data)?;

        let now = chrono::Utc::now().timestamp_millis();
        if request.expires_at <= now {
            self.storage.delete_pairing_request(code)?;
            return Err(anyhow!("Pairing request expired"));
        }

        let peer = AllowedPeer {
            peer_id: request.peer_id.clone(),
            peer_name: request.peer_name.clone(),
            approved_at: now,
            approved_by: approved_by.to_string(),
        };

        let peer_data = serde_json::to_vec(&peer)?;
        self.storage.add_peer(&peer.peer_id, &peer_data)?;
        self.storage.delete_pairing_request(code)?;

        Ok((peer, request))
    }

    /// Deny/delete a pairing request by code.
    pub fn deny(&self, code: &str) -> Result<()> {
        self.storage.delete_pairing_request(code)
    }

    /// Remove an allowed peer.
    pub fn revoke(&self, peer_id: &str) -> Result<bool> {
        self.storage.remove_peer(peer_id)
    }

    /// List all allowed peers.
    pub fn list_allowed(&self) -> Result<Vec<AllowedPeer>> {
        let raw = self.storage.list_peers()?;
        let mut peers = Vec::with_capacity(raw.len());
        for (_id, data) in raw {
            let peer: AllowedPeer = serde_json::from_slice(&data)?;
            peers.push(peer);
        }
        Ok(peers)
    }

    /// List pending pairing requests.
    pub fn list_pending(&self) -> Result<Vec<PairingRequest>> {
        let raw = self.storage.list_pairing_requests()?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut requests = Vec::new();
        for (_code, data) in raw {
            let req: PairingRequest = serde_json::from_slice(&data)?;
            if req.expires_at > now {
                requests.push(req);
            }
        }
        Ok(requests)
    }

    /// Cleanup expired requests.
    pub fn cleanup_expired(&self) -> Result<u32> {
        let now = chrono::Utc::now().timestamp_millis();
        self.storage.cleanup_expired_requests(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::NamedTempFile;

    fn create_test_manager() -> PairingManager {
        let tmp = NamedTempFile::new().unwrap();
        let db = Arc::new(Database::create(tmp.path()).unwrap());
        let storage = Arc::new(PairingStorage::new(db).unwrap());
        PairingManager::new(storage)
    }

    #[test]
    fn test_pairing_code_format() {
        let mgr = create_test_manager();
        let code = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();
        assert_eq!(code.len(), 8);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_approve_request() {
        let mgr = create_test_manager();
        let code = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();

        assert!(!mgr.is_allowed("12345").unwrap());

        let peer = mgr.approve(&code, "cli").unwrap();
        assert_eq!(peer.peer_id, "12345");
        assert_eq!(peer.peer_name, Some("Alice".to_string()));
        assert_eq!(peer.approved_by, "cli");

        assert!(mgr.is_allowed("12345").unwrap());
    }

    #[test]
    fn test_approve_with_request_preserves_chat_id() {
        let mgr = create_test_manager();
        let code = mgr
            .create_request("12345", Some("Alice"), "chat-123")
            .unwrap();

        let (peer, request) = mgr.approve_with_request(&code, "cli").unwrap();
        assert_eq!(peer.peer_id, "12345");
        assert_eq!(request.chat_id, "chat-123");
    }

    #[test]
    fn test_deny_request() {
        let mgr = create_test_manager();
        let code = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();

        mgr.deny(&code).unwrap();
        assert!(!mgr.is_allowed("12345").unwrap());

        // Approve should fail after denial
        assert!(mgr.approve(&code, "cli").is_err());
    }

    #[test]
    fn test_duplicate_request() {
        let mgr = create_test_manager();
        let code1 = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();
        // Creating another request for the same peer reuses the existing
        // pending code until expiry.
        let code2 = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();
        assert_eq!(code1, code2);

        // The existing code remains valid.
        let peer = mgr.approve(&code2, "cli").unwrap();
        assert_eq!(peer.peer_id, "12345");
    }

    #[test]
    fn test_has_pending_request() {
        let mgr = create_test_manager();
        assert!(!mgr.has_pending_request("12345").unwrap());

        let _code = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();
        assert!(mgr.has_pending_request("12345").unwrap());
    }

    #[test]
    fn test_list_allowed_peers() {
        let mgr = create_test_manager();
        let code1 = mgr.create_request("111", Some("A"), "chat-1").unwrap();
        let code2 = mgr.create_request("222", Some("B"), "chat-2").unwrap();
        mgr.approve(&code1, "cli").unwrap();
        mgr.approve(&code2, "cli").unwrap();

        let peers = mgr.list_allowed().unwrap();
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_revoke_peer() {
        let mgr = create_test_manager();
        let code = mgr
            .create_request("12345", Some("Alice"), "chat-1")
            .unwrap();
        mgr.approve(&code, "cli").unwrap();
        assert!(mgr.is_allowed("12345").unwrap());

        assert!(mgr.revoke("12345").unwrap());
        assert!(!mgr.is_allowed("12345").unwrap());
    }

    #[test]
    fn test_allow_peer_directly() {
        let mgr = create_test_manager();
        assert!(!mgr.is_allowed("999").unwrap());

        let peer = mgr.allow_peer("999", Some("Bootstrap"), "system").unwrap();
        assert_eq!(peer.peer_id, "999");
        assert_eq!(peer.peer_name, Some("Bootstrap".to_string()));
        assert_eq!(peer.approved_by, "system");
        assert!(mgr.is_allowed("999").unwrap());
    }
}
