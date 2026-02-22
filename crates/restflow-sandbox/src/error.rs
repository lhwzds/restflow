//! Sandbox error types.

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("failed to generate sandbox profile: {0}")]
    ProfileGeneration(String),

    #[error("sandbox setup failed: {0}")]
    Setup(#[from] std::io::Error),

    #[error("landlock not supported on this kernel")]
    LandlockUnsupported,
}
