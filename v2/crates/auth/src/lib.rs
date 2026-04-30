//! # codocia
//!
//! Auth owns secret references and provider access profiles.
//!
//! ## Owns
//! - SecretRef
//! - Profile
//! - provider credential references
//! - profile repository helpers
//!
//! ## Must Not
//! - expose secret values in docs
//! - define model catalog entries
//! - call UI code
//!
//! ## Inputs
//! - provider IDs
//! - secret keys
//! - profile repository
//!
//! ## Outputs
//! - auth profiles
//! - secret references
//!
//! ## Depends On
//! - model
//! - store
//!
//! ## Verify
//! - cargo check -p auth

use anyhow::Result;
use model::Provider;
use serde::{Deserialize, Serialize};
use store::Repository;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    pub key: String,
}

impl SecretRef {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub provider: Provider,
    pub secret: SecretRef,
}

impl Profile {
    pub fn new(provider: impl Into<String>, secret_key: impl Into<String>) -> Self {
        Self {
            provider: Provider::new(provider),
            secret: SecretRef::new(secret_key),
        }
    }

    pub fn id(&self) -> &str {
        &self.provider.id
    }
}

pub async fn load_profile<R>(repository: &R, provider_id: &str) -> Result<Option<Profile>>
where
    R: Repository<Profile> + ?Sized,
{
    repository.get(provider_id).await
}

pub async fn save_profile<R>(repository: &R, profile: Profile) -> Result<()>
where
    R: Repository<Profile> + ?Sized,
{
    let provider_id = profile.id().to_owned();
    repository.put(&provider_id, profile).await
}

pub async fn has_profile<R>(repository: &R, provider_id: &str) -> Result<bool>
where
    R: Repository<Profile> + ?Sized,
{
    repository.exists(provider_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use store::MemoryStore;

    #[test]
    fn profile_helpers_persist_provider_secret_refs() {
        block_on_once(async {
            let store = MemoryStore::new();
            let profile = Profile::new("openai", "OPENAI_API_KEY");

            save_profile(&store, profile.clone()).await.unwrap();

            assert!(has_profile(&store, "openai").await.unwrap());
            assert_eq!(load_profile(&store, "openai").await.unwrap(), Some(profile));
            assert!(!has_profile(&store, "missing").await.unwrap());
        });
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("auth repository future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
