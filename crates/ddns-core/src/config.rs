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

/// Trait for provider-specific configuration
///
/// This trait enables the plugin architecture where each provider
/// defines its own configuration schema and environment variable names.
/// This eliminates the need to modify ddns-core when adding new providers.
///
/// # Design Goals
///
/// - **Zero Modification**: New providers don't require changes to ddns-core
/// - **Provider-Specific Env Vars**: Each provider uses unique environment variable prefixes
/// - **Self-Validating**: Providers validate their own configuration
/// - **Type-Safe Creation**: Providers create instances from validated config
///
/// # Example
///
/// ```rust
/// struct CloudflareConfigurable;
///
/// impl ProviderConfigurable for CloudflareConfigurable {
///     fn load_from_env() -> Result<serde_json::Value> {
///         let api_token = env::var("CLOUDFLARE_API_TOKEN")
///             .map_err(|_| Error::config("CLOUDFLARE_API_TOKEN is required"))?;
///         let zone_id = env::var("CLOUDFLARE_ZONE_ID").ok();
///
///         Ok(serde_json::json!({
///             "api_token": api_token,
///             "zone_id": zone_id,
///         }))
///     }
///
///     fn validate(config: &serde_json::Value) -> Result<()> {
///         config.get("api_token")
///             .and_then(|v| v.as_str())
///             .ok_or_else(|| Error::config("Missing api_token"))?;
///         Ok(())
///     }
///
///     fn create_provider(
///         config: &serde_json::Value,
///         dry_run: bool,
///     ) -> Result<Box<dyn DnsProvider>> {
///         let api_token = config.get("api_token").and_then(|v| v.as_str()).unwrap();
///         let zone_id = config.get("zone_id").and_then(|v| v.as_str()).map(|s| s.to_string());
///         Ok(Box::new(CloudflareProvider::new(api_token, zone_id, dry_run)))
///     }
///
///     fn provider_name() -> &'static str {
///         "cloudflare"
///     }
/// }
/// ```
pub trait ProviderConfigurable: Send + Sync {
    /// Get provider name (for logging and registration)
    ///
    /// This is an instance method that can be called on trait objects.
    ///
    /// # Returns
    ///
    /// Provider name as static string (e.g., "cloudflare", "aliyun")
    fn name(&self) -> &'static str;

    /// Load provider configuration from environment variables
    ///
    /// This method is called by the daemon to load provider-specific
    /// configuration. Each provider can define its own environment
    /// variable naming convention (e.g., `CLOUDFLARE_API_TOKEN`,
    /// `ALIYUN_ACCESS_KEY_ID`, etc.).
    ///
    /// # Returns
    ///
    /// Configuration data as JSON (can be any JSON-serializable type)
    ///
    /// # Errors
    ///
    /// Returns error if required environment variables are missing or invalid
    fn load_from_env(&self) -> Result<serde_json::Value, crate::Error>;

    /// Validate provider configuration
    ///
    /// This method validates the configuration data loaded from
    /// environment variables. Providers should check for required
    /// fields and validate their formats.
    ///
    /// # Parameters
    ///
    /// - `config`: Configuration data loaded from environment
    ///
    /// # Returns
    ///
    /// Ok(()) if valid, Error otherwise
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid
    fn validate(&self, config: &serde_json::Value) -> Result<(), crate::Error>;

    /// Create provider instance from configuration
    ///
    /// This method constructs the actual provider instance from
    /// validated configuration data.
    ///
    /// # Parameters
    ///
    /// - `config`: Configuration data (already validated)
    /// - `dry_run`: Whether to run in dry-run mode
    ///
    /// # Returns
    ///
    /// Boxed DnsProvider trait object
    ///
    /// # Errors
    ///
    /// Returns error if provider creation fails
    fn create_provider(
        &self,
        config: &serde_json::Value,
        dry_run: bool,
    ) -> Result<Box<dyn crate::traits::DnsProvider>, crate::Error>;
}

/// DNS provider configuration (wrapper for plugin system)
///
/// This struct replaces the enum-based configuration with a flexible
/// JSON-based approach. Each provider can define its own configuration
/// schema through the `ProviderConfigurable` trait.
///
/// # Design Benefits
///
/// - **Zero Modification**: New providers don't require changes to this struct
/// - **Flexible Schema**: Each provider defines its own config structure
/// - **JSON Serializable**: Works seamlessly with serde
/// - **Type Safe**: Providers validate their own config via trait
///
/// # Example
///
/// ```rust
/// // Cloudflare provider config
/// let config = ProviderConfig {
///     provider_type: "cloudflare".to_string(),
///     config: serde_json::json!({
///         "api_token": "my_token",
///         "zone_id": "my_zone",
///     }),
/// };
///
/// // NameSilo provider config
/// let config = ProviderConfig {
///     provider_type: "namesilo".to_string(),
///     config: serde_json::json!({
///         "api_key": "my_key",
///     }),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type name (e.g., "cloudflare", "aliyun", "namesilo")
    ///
    /// This identifies which provider should be instantiated.
    /// The name must match a registered provider in the ProviderRegistry.
    #[serde(rename = "type")]
    pub provider_type: String,

    /// Provider-specific configuration data
    ///
    /// Each provider can define its own schema for this data.
    /// The provider's `ProviderConfigurable` implementation will
    /// validate and interpret this data.
    ///
    /// # Common Patterns
    ///
    /// - **Cloudflare**: `{"api_token": "...", "zone_id": "..."}`
    /// - **Aliyun**: `{"access_key_id": "...", "access_key_secret": "..."}`
    /// - **NameSilo**: `{"api_key": "..."}`
    /// - **GoDaddy**: `{"api_key": "...", "api_secret": "..."}`
    #[serde(default = "default_provider_config")]
    pub config: serde_json::Value,
}

impl ProviderConfig {
    /// Validate the provider configuration
    ///
    /// Note: This is a minimal validation. Full validation is performed
    /// by the provider's `ProviderConfigurable::validate()` implementation.
    pub fn validate(&self) -> Result<(), crate::Error> {
        if self.provider_type.is_empty() {
            return Err(crate::Error::config("Provider type cannot be empty"));
        }
        if self.config.is_null() {
            return Err(crate::Error::config("Provider config cannot be null"));
        }
        Ok(())
    }

    /// Get the provider type name
    pub fn type_name(&self) -> &str {
        &self.provider_type
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        ProviderConfig {
            provider_type: "cloudflare".to_string(),
            config: serde_json::json!({}),
        }
    }
}

fn default_provider_config() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
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
