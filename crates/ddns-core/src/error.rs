//! Error types for the DDNS system
//!
//! This module defines all error types used throughout the crate.

use thiserror::Error;

/// Result type alias for DDNS operations
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type for the DDNS system
#[derive(Error, Debug)]
pub enum Error {
    /// IP source-related errors
    #[error("IP source error: {0}")]
    IpSource(String),

    /// DNS provider-related errors
    #[error("DNS provider error: {0}")]
    DnsProvider(String),

    /// State store-related errors
    #[error("State store error: {0}")]
    StateStore(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Network-related errors
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP client errors (from provider APIs)
    #[error("HTTP error: {0}")]
    Http(String),

    /// Authentication errors
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Rate limiting errors
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// Record not found
    #[error("Record not found: {0}")]
    NotFound(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Provider-specific error
    #[error("Provider error ({provider}): {message}")]
    Provider {
        /// Provider name
        provider: String,
        /// Error message
        message: String,
    },

    /// Generic error with context
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Create an IP source error
    pub fn ip_source(msg: impl Into<String>) -> Self {
        Self::IpSource(msg.into())
    }

    /// Create a DNS provider error
    pub fn dns_provider(msg: impl Into<String>) -> Self {
        Self::DnsProvider(msg.into())
    }

    /// Create a state store error
    pub fn state_store(msg: impl Into<String>) -> Self {
        Self::StateStore(msg.into())
    }

    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create an HTTP error
    pub fn http(msg: impl Into<String>) -> Self {
        Self::Http(msg.into())
    }

    /// Create an authentication error
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Authentication(msg.into())
    }

    /// Create a rate limit error
    pub fn rate_limited(msg: impl Into<String>) -> Self {
        Self::RateLimited(msg.into())
    }

    /// Create a "not found" error
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Create an invalid input error
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Create a provider-specific error
    pub fn provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Provider {
            provider: provider.into(),
            message: message.into(),
        }
    }
}

/// Helper for converting anyhow::Error to our Error type
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err.to_string())
    }
}
