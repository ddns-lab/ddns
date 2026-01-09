//! Configuration types for the DDNS system
//!
//! This module defines all configuration structures used throughout the crate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main DDNS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdnsConfig {
    /// IP source configuration
    pub ip_source: IpSourceConfig,

    /// DNS provider configuration
    pub provider: ProviderConfig,

    /// State store configuration
    pub state_store: StateStoreConfig,

    /// DNS records to manage
    pub records: Vec<RecordConfig>,

    /// Optional engine settings
    #[serde(default)]
    pub engine: EngineConfig,
}

impl DdnsConfig {
    /// Create a new configuration with defaults
    pub fn new() -> Self {
        Self {
            ip_source: IpSourceConfig::default(),
            provider: ProviderConfig::default(),
            state_store: StateStoreConfig::default(),
            records: Vec::new(),
            engine: EngineConfig::default(),
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), crate::Error> {
        if self.records.is_empty() {
            return Err(crate::Error::config("No records configured"));
        }

        self.provider.validate()?;
        self.ip_source.validate()?;

        Ok(())
    }
}

impl Default for DdnsConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// IP source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpSourceConfig {
    /// Netlink-based IP source (Linux)
    Netlink {
        /// Network interface to monitor (e.g., "eth0")
        interface: Option<String>,
        /// IP version to monitor (v4, v6, or both)
        version: Option<IpVersion>,
    },

    /// HTTP-based IP source (uses external service)
    Http {
        /// URL to fetch IP from
        url: String,
        /// Request interval in seconds
        interval_secs: u64,
    },

    /// Custom IP source
    Custom {
        /// Factory name to use
        factory: String,
        /// Custom configuration data
        config: serde_json::Value,
    },
}

impl IpSourceConfig {
    /// Validate the IP source configuration
    pub fn validate(&self) -> Result<(), crate::Error> {
        match self {
            IpSourceConfig::Http { url, interval_secs } => {
                if url.is_empty() {
                    return Err(crate::Error::config("HTTP IP source URL cannot be empty"));
                }
                if *interval_secs == 0 {
                    return Err(crate::Error::config("HTTP IP source interval must be > 0"));
                }
                Ok(())
            }
            IpSourceConfig::Custom { factory, config } => {
                if factory.is_empty() {
                    return Err(crate::Error::config(
                        "Custom IP source factory cannot be empty",
                    ));
                }
                if config.is_null() {
                    return Err(crate::Error::config(
                        "Custom IP source config cannot be null",
                    ));
                }
                Ok(())
            }
            IpSourceConfig::Netlink { .. } => Ok(()),
        }
    }
}

impl Default for IpSourceConfig {
    fn default() -> Self {
        IpSourceConfig::Netlink {
            interface: None,
            version: None,
        }
    }
}

/// IP version to monitor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IpVersion {
    /// IPv4 only
    V4,
    /// IPv6 only
    V6,
    /// Both IPv4 and IPv6
    Both,
}

/// DNS provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderConfig {
    /// Cloudflare provider
    Cloudflare {
        /// Cloudflare API token
        api_token: String,
        /// Zone ID (optional, can be auto-detected)
        zone_id: Option<String>,
        /// Account ID (optional)
        account_id: Option<String>,
    },

    /// Custom provider
    Custom {
        /// Factory name to use
        factory: String,
        /// Custom configuration data
        config: serde_json::Value,
    },
}

impl ProviderConfig {
    /// Validate the provider configuration
    pub fn validate(&self) -> Result<(), crate::Error> {
        match self {
            ProviderConfig::Cloudflare { api_token, .. } => {
                if api_token.is_empty() {
                    return Err(crate::Error::config("Cloudflare API token cannot be empty"));
                }
                Ok(())
            }
            ProviderConfig::Custom { factory, config } => {
                if factory.is_empty() {
                    return Err(crate::Error::config(
                        "Custom provider factory cannot be empty",
                    ));
                }
                if config.is_null() {
                    return Err(crate::Error::config(
                        "Custom provider config cannot be null",
                    ));
                }
                Ok(())
            }
        }
    }

    /// Get the provider type name
    pub fn type_name(&self) -> &str {
        match self {
            ProviderConfig::Cloudflare { .. } => "cloudflare",
            ProviderConfig::Custom { factory, .. } => factory,
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        ProviderConfig::Cloudflare {
            api_token: String::new(),
            zone_id: None,
            account_id: None,
        }
    }
}

/// State store configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateStoreConfig {
    /// File-based state store
    File {
        /// Path to the state file
        path: String,
    },

    /// In-memory state store (not persistent)
    #[default]
    Memory,

    /// Custom state store
    Custom {
        /// Factory name to use
        factory: String,
        /// Custom configuration data
        config: serde_json::Value,
    },
}

/// DNS record configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordConfig {
    /// DNS record name (e.g., "example.com" or "sub.example.com")
    pub name: String,

    /// Record type (A for IPv4, AAAA for IPv6, or auto-detect)
    #[serde(default = "default_record_type")]
    pub record_type: RecordType,

    /// Whether this record is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl RecordConfig {
    /// Create a new record configuration
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            record_type: RecordType::Auto,
            enabled: true,
        }
    }

    /// Set the record type
    pub fn with_record_type(mut self, record_type: RecordType) -> Self {
        self.record_type = record_type;
        self
    }

    /// Enable or disable the record
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// DNS record type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordType {
    /// A record (IPv4)
    A,
    /// AAAA record (IPv6)
    Aaaa,
    /// Auto-detect based on IP version
    Auto,
}

fn default_record_type() -> RecordType {
    RecordType::Auto
}

fn default_enabled() -> bool {
    true
}

/// Engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Maximum number of retry attempts for failed updates
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,

    /// Delay between retry attempts (in seconds)
    #[serde(default = "default_retry_delay_secs")]
    pub retry_delay_secs: u64,

    /// Initial startup delay (in seconds)
    #[serde(default = "default_startup_delay_secs")]
    pub startup_delay_secs: u64,

    /// Minimum interval between DNS updates for the same record (in seconds)
    ///
    /// This prevents IP flapping from causing excessive API calls.
    /// If the IP changes multiple times within this interval, only the last
    /// IP will trigger a DNS update.
    ///
    /// Set to 0 to disable rate limiting (not recommended for production).
    #[serde(default = "default_min_update_interval_secs")]
    pub min_update_interval_secs: u64,

    /// Capacity of the internal event channel
    ///
    /// When full, new IP change events will be dropped (with a warning log).
    /// This prevents unbounded memory growth under high IP churn.
    ///
    /// Default: 1000 events
    #[serde(default = "default_event_channel_capacity")]
    pub event_channel_capacity: usize,

    /// Additional metadata to attach to operations
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_delay_secs: default_retry_delay_secs(),
            startup_delay_secs: default_startup_delay_secs(),
            min_update_interval_secs: default_min_update_interval_secs(),
            event_channel_capacity: default_event_channel_capacity(),
            metadata: HashMap::new(),
        }
    }
}

fn default_max_retries() -> usize {
    3
}

fn default_retry_delay_secs() -> u64 {
    5
}

fn default_min_update_interval_secs() -> u64 {
    60
}

fn default_event_channel_capacity() -> usize {
    1000
}

fn default_startup_delay_secs() -> u64 {
    0
}
