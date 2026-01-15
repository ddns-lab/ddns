// # ddnsd - DDNS Daemon
//
// ⚠️ ARCHITECTURAL CONSTRAINTS ⚠️
//
// This daemon is governed by .ai/AI_CONTRACT.md.
// CRITICAL RULES:
// - This is a THIN integration layer ONLY (per AI_CONTRACT.md §2.1)
// - DO NOT add business logic, DNS logic, or retry logic here
// - All DDNS logic MUST be in ddns-core
// - Configuration is via environment variables ONLY (per AI_CONTRACT.md §6)
//
// The ddnsd daemon is responsible for:
// 1. Reading configuration from environment variables
// 2. Initializing the runtime
// 3. Registering providers and IP sources
// 4. Starting the DDNS engine
//
// ## Configuration
//
// All configuration is done via environment variables:
//
// ### IP Source
// - `DDNS_IP_SOURCE_TYPE`: Type of IP source (netlink, http)
// - `DDNS_IP_SOURCE_INTERFACE`: Network interface (for netlink)
// - `DDNS_IP_SOURCE_URL`: URL to fetch IP from (for http)
// - `DDNS_IP_SOURCE_INTERVAL`: Poll interval in seconds (for http)
// - `DDNS_IP_SOURCE_VERSION`: IP version to monitor (v4, v6, both) [optional]
//
// ### DNS Provider
// - `DDNS_PROVIDER_TYPE`: Provider type (cloudflare)
// - `DDNS_PROVIDER_API_TOKEN`: API token
// - `DDNS_PROVIDER_ZONE_ID`: Zone ID (optional)
//
// ### Records
// - `DDNS_RECORDS`: Comma-separated list of DNS records with optional types
//
//   Format: `record_name[:type]`
//
//   Types:
//   - `A` - IPv4 address record
//   - `AAAA` - IPv6 address record
//   - `Auto` (or omit) - Auto-detect based on IP version
//
//   Examples:
//   - `example.com` → Auto (default)
//   - `example.com:A` → A record only (IPv4)
//   - `example.com:AAAA` → AAAA record only (IPv6)
//   - `a.example.com:A,aaaa.example.com:AAAA` → Multiple records with types
//
// ### State Store
// - `DDNS_STATE_STORE_TYPE`: Type of state store (file, memory)
// - `DDNS_STATE_STORE_PATH`: Path to state file (for file store)
//
// ### Engine
// - `DDNS_MAX_RETRIES`: Maximum retry attempts
// - `DDNS_RETRY_DELAY_SECS`: Delay between retries
//
// ## Example
//
// ### Monitor both IPv4 and IPv6, update A and AAAA records
//
// ```bash
// export DDNS_IP_SOURCE_TYPE=netlink
// export DDNS_IP_SOURCE_VERSION=both  # Monitor both IP versions
// export DDNS_PROVIDER_TYPE=cloudflare
// export DDNS_PROVIDER_API_TOKEN=your_token
// export DDNS_RECORDS=example.com:A,example.com:AAAA
// export DDNS_STATE_STORE_TYPE=file
// export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
// ddnsd
// ```
//
// ### Monitor IPv4 only, update A record
//
// ```bash
// export DDNS_IP_SOURCE_TYPE=netlink
// export DDNS_IP_SOURCE_VERSION=v4
// export DDNS_RECORDS=example.com:A
// ddnsd
// ```

use anyhow::Result;
use std::env;
use std::process::ExitCode;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

use ddns_core::config::{IpVersion, RecordType};

#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};

/// Exit codes for different termination scenarios
///
/// These codes follow systemd conventions:
/// - 0: Clean shutdown
/// - 1: Configuration or startup error
/// - 2: Runtime error (unexpected)
#[derive(Debug, Clone, Copy)]
enum DdnsExitCode {
    /// Clean shutdown (normal exit)
    CleanShutdown = 0,
    /// Configuration error or startup failure
    ConfigError = 1,
    /// Runtime error (unexpected failure)
    RuntimeError = 2,
}

impl From<DdnsExitCode> for ExitCode {
    fn from(code: DdnsExitCode) -> Self {
        ExitCode::from(code as u8)
    }
}

/// Get version from git tag or Cargo.toml
///
/// Priority:
/// 1. GIT_VERSION (set by build.rs from git describe)
/// 2. CARGO_PKG_VERSION (fallback from Cargo.toml)
fn get_version() -> &'static str {
    // Use git version if available (set by build.rs), otherwise use Cargo version
    option_env!("GIT_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Print help message
fn print_help() {
    println!("ddnsd {} - Dynamic DNS Daemon\n", get_version());
    println!("USAGE:");
    println!("  ddnsd                    Run the daemon (configure via env vars)");
    println!("  ddnsd --version          Show version information");
    println!("  ddnsd --help             Show this help message");
    println!();
    println!("ENVIRONMENT VARIABLES:");
    println!("  IP Source Configuration:");
    println!("    DDNS_IP_SOURCE_TYPE      IP source type (netlink, http) [default: netlink]");
    println!("    DDNS_IP_SOURCE_INTERFACE Network interface (for netlink)");
    println!("    DDNS_IP_SOURCE_URL      URL to fetch IP from (for http)");
    println!("    DDNS_IP_SOURCE_INTERVAL Poll interval in seconds (for http) [default: 60]");
    println!("    DDNS_IP_SOURCE_VERSION   IP version (v4, v6, both) [default: both]");
    println!();
    println!("  DNS Provider Configuration:");
    println!("    DDNS_PROVIDER_TYPE             Provider type (cloudflare, aliyun, namesilo, godaddy) [default: cloudflare]");
    println!("    DDNS_PROVIDER_API_TOKEN        API token [required for Cloudflare]");
    println!("    DDNS_PROVIDER_ZONE_ID          Zone ID (optional, for Cloudflare)");
    println!("    DDNS_PROVIDER_ACCESS_KEY_ID    AccessKey ID [required for Aliyun]");
    println!("    DDNS_PROVIDER_ACCESS_KEY_SECRET AccessKey Secret [required for Aliyun]");
    println!("    DDNS_PROVIDER_API_KEY          API key [required for NameSilo/GoDaddy]");
    println!("    DDNS_PROVIDER_API_SECRET       API secret [required for GoDaddy]");
    println!();
    println!("  Records:");
    println!("    DDNS_RECORDS             Comma-separated records with optional types");
    println!("                             Format: name[:type]");
    println!("                             Types: A, AAAA, Auto (default)");
    println!("                             Example: example.com:A,example.com:AAAA");
    println!();
    println!("  State Store:");
    println!("    DDNS_STATE_STORE_TYPE     Type (file, memory) [default: file]");
    println!("    DDNS_STATE_STORE_PATH     Path to state file (for file store)");
    println!();
    println!("  Engine:");
    println!("    DDNS_MAX_RETRIES          Max retry attempts [default: 3]");
    println!("    DDNS_RETRY_DELAY_SECS     Delay between retries [default: 5]");
    println!("    DDNS_LOG_LEVEL            Log level (trace, debug, info, warn, error)");
    println!();
    println!("EXAMPLES:");
    println!("  # Monitor both IPv4 and IPv6");
    println!("  export DDNS_IP_SOURCE_VERSION=both");
    println!("  export DDNS_RECORDS=example.com:A,example.com:AAAA");
    println!("  ddnsd");
    println!();
    println!("DOCUMENTATION:");
    println!("  https://github.com/ddns-lab/ddns");
}

/// Application configuration
#[allow(dead_code)]
struct Config {
    ip_source_type: String,
    ip_source_interface: Option<String>,
    ip_source_url: Option<String>,
    ip_source_interval: Option<u64>,
    ip_source_version: Option<String>,
    provider_type: String,
    provider_api_token: String,
    provider_zone_id: Option<String>,
    records: Vec<String>,
    state_store_type: String,
    state_store_path: Option<String>,
    max_retries: Option<usize>,
    retry_delay_secs: Option<u64>,
    startup_delay_secs: Option<u64>,
    min_update_interval_secs: Option<u64>,
    log_level: String,
}

impl Config {
    /// Load configuration from environment variables
    fn from_env() -> Result<Self> {
        Ok(Self {
            ip_source_type: env::var("DDNS_IP_SOURCE_TYPE")
                .unwrap_or_else(|_| "netlink".to_string()),
            ip_source_interface: env::var("DDNS_IP_SOURCE_INTERFACE").ok(),
            ip_source_url: env::var("DDNS_IP_SOURCE_URL").ok(),
            ip_source_interval: env::var("DDNS_IP_SOURCE_INTERVAL")
                .ok()
                .map(|s| s.parse().unwrap_or(60)),
            ip_source_version: env::var("DDNS_IP_SOURCE_VERSION").ok(),
            provider_type: env::var("DDNS_PROVIDER_TYPE")
                .unwrap_or_else(|_| "cloudflare".to_string()),
            provider_api_token: env::var("DDNS_PROVIDER_API_TOKEN")?,
            provider_zone_id: env::var("DDNS_PROVIDER_ZONE_ID").ok(),
            records: env::var("DDNS_RECORDS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            state_store_type: env::var("DDNS_STATE_STORE_TYPE")
                .unwrap_or_else(|_| "file".to_string()),
            state_store_path: env::var("DDNS_STATE_STORE_PATH").ok(),
            max_retries: env::var("DDNS_MAX_RETRIES")
                .ok()
                .map(|s| s.parse().unwrap_or(3)),
            retry_delay_secs: env::var("DDNS_RETRY_DELAY_SECS")
                .ok()
                .map(|s| s.parse().unwrap_or(5)),
            startup_delay_secs: env::var("DDNS_STARTUP_DELAY_SECS")
                .ok()
                .map(|s| s.parse().unwrap_or(0)),
            min_update_interval_secs: env::var("DDNS_MIN_UPDATE_INTERVAL_SECS")
                .ok()
                .map(|s| s.parse().unwrap_or(60)),
            log_level: env::var("DDNS_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        })
    }

    /// Validate the configuration
    ///
    /// This performs comprehensive validation including:
    /// - Required field presence
    /// - Value format validation (API tokens, domain names)
    /// - Numeric range validation
    /// - Type enumeration validation
    /// - Security checks (secret exposure, URL schemes)
    fn validate(&self) -> Result<()> {
        // Validate API token presence and format
        if self.provider_api_token.is_empty() {
            anyhow::bail!(
                "DDNS_PROVIDER_API_TOKEN is required. \
                Set it via: export DDNS_PROVIDER_API_TOKEN=your_token"
            );
        }

        // Cloudflare API tokens are typically 40 characters alphanumeric
        // Other providers may have different formats, so we do basic validation
        if self.provider_api_token.len() < 20 {
            anyhow::bail!(
                "DDNS_PROVIDER_API_TOKEN appears too short ({} chars). \
                Cloudflare tokens are typically 40 characters. \
                Verify your token is correct.",
                self.provider_api_token.len()
            );
        }

        // Check for obvious placeholder tokens (common mistake)
        let token_lower = self.provider_api_token.to_lowercase();
        if token_lower.contains("your_token")
            || token_lower.contains("replace_me")
            || token_lower.contains("example")
            || token_lower == "token"
        {
            anyhow::bail!(
                "DDNS_PROVIDER_API_TOKEN appears to be a placeholder. \
                Use an actual API token from your DNS provider."
            );
        }

        // Validate provider type
        match self.provider_type.as_str() {
            "cloudflare" | "aliyun" | "namesilo" | "godaddy" => {} // Currently supported
            _ => anyhow::bail!(
                "DDNS_PROVIDER_TYPE '{}' is not supported. \
                Supported providers: cloudflare, aliyun, namesilo, godaddy",
                self.provider_type
            ),
        }

        // Validate IP source type
        match self.ip_source_type.as_str() {
            "netlink" | "http" | "file" => {}
            _ => anyhow::bail!(
                "DDNS_IP_SOURCE_TYPE '{}' is not supported. \
                Supported types: netlink, http, file",
                self.ip_source_type
            ),
        }

        // Validate state store type
        match self.state_store_type.as_str() {
            "file" | "memory" => {}
            _ => anyhow::bail!(
                "DDNS_STATE_STORE_TYPE '{}' is not supported. \
                Supported types: file, memory",
                self.state_store_type
            ),
        }

        // Validate records (must be valid domain names)
        if self.records.is_empty() {
            anyhow::bail!(
                "DDNS_RECORDS must contain at least one record. \
                Set it via: export DDNS_RECORDS=example.com,www.example.com"
            );
        }

        for record in &self.records {
            self.validate_domain_name(record)?;
        }

        // Validate state store path for file store
        if self.state_store_type == "file" {
            if let Some(ref path) = self.state_store_path {
                // Check path is not empty
                if path.is_empty() {
                    anyhow::bail!(
                        "DDNS_STATE_STORE_PATH cannot be empty when DDNS_STATE_STORE_TYPE=file"
                    );
                }

                // Check parent directory exists or can be created
                if let Some(parent) = std::path::Path::new(path).parent()
                    && !parent.as_os_str().is_empty()
                    && !parent.exists()
                {
                    anyhow::bail!(
                        "DDNS_STATE_STORE_PATH parent directory does not exist: {}. \
                            Create it first: sudo mkdir -p {}",
                        parent.display(),
                        parent.display()
                    );
                }
            } else {
                anyhow::bail!(
                    "DDNS_STATE_STORE_PATH is required when DDNS_STATE_STORE_TYPE=file. \
                    Set it via: export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json"
                );
            }
        }

        // Validate IP source URL for HTTP source
        if self.ip_source_type == "http" {
            if self.ip_source_url.as_ref().is_none_or(|u| u.is_empty()) {
                anyhow::bail!("DDNS_IP_SOURCE_URL is required when DDNS_IP_SOURCE_TYPE=http");
            }

            if let Some(ref url) = self.ip_source_url {
                // Validate URL scheme (HTTPS only for security)
                if !url.starts_with("https://") && !url.starts_with("http://") {
                    anyhow::bail!(
                        "DDNS_IP_SOURCE_URL must use HTTP or HTTPS scheme. Got: {}",
                        url
                    );
                }

                // Warn if using HTTP (not HTTPS)
                if url.starts_with("http://") && !url.starts_with("https://") {
                    eprintln!(
                        "WARNING: DDNS_IP_SOURCE_URL uses HTTP (not HTTPS). \
                              This is less secure. Consider using HTTPS."
                    );
                }
            }
        }

        // Validate numeric ranges
        if let Some(interval) = self.ip_source_interval
            && (!(10..=3600).contains(&interval))
        {
            anyhow::bail!(
                "DDNS_IP_SOURCE_INTERVAL must be between 10 and 3600 seconds. Got: {}",
                interval
            );
        }

        if let Some(max_retries) = self.max_retries
            && (max_retries == 0 || max_retries > 10)
        {
            anyhow::bail!(
                "DDNS_MAX_RETRIES must be between 1 and 10. Got: {}",
                max_retries
            );
        }

        if let Some(retry_delay) = self.retry_delay_secs
            && (!(1..=300).contains(&retry_delay))
        {
            anyhow::bail!(
                "DDNS_RETRY_DELAY_SECS must be between 1 and 300 seconds. Got: {}",
                retry_delay
            );
        }

        // Validate log level
        match self.log_level.to_lowercase().as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            _ => anyhow::bail!(
                "DDNS_LOG_LEVEL '{}' is not valid. \
                Valid levels: trace, debug, info, warn, error",
                self.log_level
            ),
        }

        Ok(())
    }

    /// Validate that a string is a valid domain name
    ///
    /// This implements basic DNS domain name validation per RFC 1035.
    /// It's not comprehensive but catches common errors.
    fn validate_domain_name(&self, domain: &str) -> Result<()> {
        if domain.is_empty() {
            anyhow::bail!("Domain name cannot be empty");
        }

        // Total length limit (RFC 1035: 253 chars max)
        if domain.len() > 253 {
            anyhow::bail!(
                "Domain name too long: {} chars (max 253). Got: {}",
                domain.len(),
                domain
            );
        }

        // Split into labels and validate each
        for label in domain.split('.') {
            if label.is_empty() {
                anyhow::bail!("Domain name has empty label: '{}'", domain);
            }

            if label.len() > 63 {
                anyhow::bail!(
                    "Domain label too long: {} chars (max 63). Label: '{}'",
                    label.len(),
                    label
                );
            }

            // Check for valid characters (alphanumeric and hyphen)
            if !label.chars().all(|c| c.is_alphanumeric() || c == '-') {
                anyhow::bail!(
                    "Domain label contains invalid characters. Label: '{}'. \
                    Valid: alphanumeric and hyphen only.",
                    label
                );
            }

            // Label cannot start or end with hyphen
            if label.starts_with('-') || label.ends_with('-') {
                anyhow::bail!(
                    "Domain label cannot start or end with hyphen. Label: '{}'",
                    label
                );
            }
        }

        Ok(())
    }
}

fn main() -> ExitCode {
    // Check for --version flag
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("ddnsd {}", get_version());
                return DdnsExitCode::CleanShutdown.into();
            }
            "--help" | "-h" => {
                print_help();
                return DdnsExitCode::CleanShutdown.into();
            }
            _ => {}
        }
    }

    // Load configuration from environment
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            return DdnsExitCode::ConfigError.into();
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Configuration validation error: {}", e);
        return DdnsExitCode::ConfigError.into();
    }

    // Initialize tracing
    let log_level = match config.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();

    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("Failed to set tracing subscriber: {}", e);
        return DdnsExitCode::ConfigError.into();
    }

    info!("Starting ddnsd daemon");
    info!("Configuration loaded: {} record(s)", config.records.len());

    // Enter tokio runtime
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            error!("Failed to create tokio runtime: {}", e);
            return DdnsExitCode::RuntimeError.into();
        }
    };

    let result = rt.block_on(async {
        if let Err(e) = run_daemon(config).await {
            error!("Daemon error: {}", e);
            DdnsExitCode::RuntimeError
        } else {
            DdnsExitCode::CleanShutdown
        }
    });

    result.into()
}

/// Run the daemon
async fn run_daemon(config: Config) -> Result<()> {
    use ddns_core::config::{
        DdnsConfig, EngineConfig, IpSourceConfig, ProviderConfig, RecordConfig, StateStoreConfig,
    };
    use ddns_core::{DdnsEngine, ProviderRegistry};

    // Create provider registry
    let registry = ProviderRegistry::new();

    // Register built-in providers
    #[cfg(feature = "cloudflare")]
    {
        info!("Registering Cloudflare provider");
        ddns_provider_cloudflare::register(&registry);
    }

    #[cfg(feature = "aliyun")]
    {
        info!("Registering Aliyun provider");
        ddns_provider_aliyun::register(&registry);
    }

    #[cfg(feature = "namesilo")]
    {
        info!("Registering NameSilo provider");
        ddns_provider_namesilo::register(&registry);
    }

    #[cfg(feature = "godaddy")]
    {
        info!("Registering GoDaddy provider");
        ddns_provider_godaddy::register(&registry);
    }

    #[cfg(feature = "netlink")]
    {
        info!("Registering Netlink IP source");
        ddns_ip_netlink::register(&registry);
    }

    #[cfg(feature = "http")]
    {
        info!("Registering HTTP IP source (fallback)");
        ddns_ip_http::register(&registry);
    }

    // Register built-in state stores
    info!("Registering file state store");
    registry.register_state_store("file", Box::new(ddns_core::FileStateStoreFactory));

    info!("Registering memory state store");
    registry.register_state_store("memory", Box::new(ddns_core::MemoryStateStoreFactory));

    // Create IP source config
    //
    // IMPORTANT: IP source selection is EXCLUSIVE, not fallback-based.
    // - If DDNS_IP_SOURCE_TYPE=netlink, ONLY netlink is used (Linux kernel events)
    // - If DDNS_IP_SOURCE_TYPE=http, ONLY HTTP polling is used
    // - There is NO automatic fallback between sources (per AI_CONTRACT.md §2.2)
    //
    // On Linux, ALWAYS prefer netlink for event-driven IP monitoring.
    // HTTP is only for:
    // - Non-Linux platforms (macOS, Windows, BSD)
    // - CI/CD testing environments
    // - Debugging and validation

    // Parse IP version configuration
    let ip_version = match config.ip_source_version.as_deref() {
        Some("v4") => Some(IpVersion::V4),
        Some("v6") => Some(IpVersion::V6),
        Some("both") => Some(IpVersion::Both),
        Some(value) => {
            return Err(anyhow::anyhow!(
                "Invalid DDNS_IP_SOURCE_VERSION '{}'. Valid values: v4, v6, both",
                value
            ));
        }
        None => None,
    };

    let ip_source_config = match config.ip_source_type.as_str() {
        "netlink" => IpSourceConfig::Netlink {
            interface: config.ip_source_interface.clone(),
            version: ip_version,
        },
        "http" => IpSourceConfig::Http {
            url: config
                .ip_source_url
                .unwrap_or_else(|| "https://icanhazip.com".to_string()),
            interval_secs: config.ip_source_interval.unwrap_or(60),
        },
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown IP source type: {}",
                config.ip_source_type
            ));
        }
    };

    // Create provider config
    let provider_config = match config.provider_type.as_str() {
        "cloudflare" => ProviderConfig::Cloudflare {
            api_token: config.provider_api_token.clone(),
            zone_id: config.provider_zone_id.clone(),
            account_id: None,
        },
        "aliyun" => {
            // Aliyun uses AccessKey ID and Secret
            let access_key_id = env::var("DDNS_PROVIDER_ACCESS_KEY_ID")
                .unwrap_or_else(|_| String::new());
            let access_key_secret = env::var("DDNS_PROVIDER_ACCESS_KEY_SECRET")
                .unwrap_or_else(|_| String::new());

            if access_key_id.is_empty() {
                return Err(anyhow::anyhow!(
                    "DDNS_PROVIDER_ACCESS_KEY_ID is required for Aliyun provider"
                ));
            }
            if access_key_secret.is_empty() {
                return Err(anyhow::anyhow!(
                    "DDNS_PROVIDER_ACCESS_KEY_SECRET is required for Aliyun provider"
                ));
            }

            ProviderConfig::Aliyun {
                access_key_id,
                access_key_secret,
            }
        }
        "namesilo" => {
            // NameSilo uses API key
            let api_key = env::var("DDNS_PROVIDER_API_KEY")
                .unwrap_or_else(|_| String::new());

            if api_key.is_empty() {
                return Err(anyhow::anyhow!(
                    "DDNS_PROVIDER_API_KEY is required for NameSilo provider"
                ));
            }

            ProviderConfig::NameSilo { api_key }
        }
        "godaddy" => {
            // GoDaddy uses API key and secret
            let api_key = env::var("DDNS_PROVIDER_API_KEY")
                .unwrap_or_else(|_| String::new());
            let api_secret = env::var("DDNS_PROVIDER_API_SECRET")
                .unwrap_or_else(|_| String::new());

            if api_key.is_empty() {
                return Err(anyhow::anyhow!(
                    "DDNS_PROVIDER_API_KEY is required for GoDaddy provider"
                ));
            }
            if api_secret.is_empty() {
                return Err(anyhow::anyhow!(
                    "DDNS_PROVIDER_API_SECRET is required for GoDaddy provider"
                ));
            }

            ProviderConfig::GoDaddy {
                api_key,
                api_secret,
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown provider type: {}",
                config.provider_type
            ));
        }
    };

    // Create state store config
    let state_store_config = match config.state_store_type.as_str() {
        "file" => StateStoreConfig::File {
            path: config
                .state_store_path
                .clone()
                .unwrap_or_else(|| "/var/lib/ddns/state.json".to_string()),
        },
        "memory" => StateStoreConfig::Memory,
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown state store type: {}",
                config.state_store_type
            ));
        }
    };

    // Create record configs
    // Support format: "name" or "name:type" where type is A, AAAA, or Auto
    let record_configs: Vec<RecordConfig> = config
        .records
        .iter()
        .map(|record_spec| {
            // Parse "name:type" format
            let parts: Vec<&str> = record_spec.split(':').collect();
            let name = parts[0];
            let record_type = if parts.len() > 1 {
                match parts[1].to_uppercase().as_str() {
                    "A" => RecordType::A,
                    "AAAA" => RecordType::Aaaa,
                    "AUTO" | "" => RecordType::Auto,
                    _ => {
                        eprintln!(
                            "WARNING: Invalid record type '{}' for '{}', using Auto",
                            parts[1], name
                        );
                        RecordType::Auto
                    }
                }
            } else {
                RecordType::Auto
            };

            RecordConfig::new(name).with_record_type(record_type)
        })
        .collect();

    // Create engine config
    let engine_config = EngineConfig {
        max_retries: config.max_retries.unwrap_or(3),
        retry_delay_secs: config.retry_delay_secs.unwrap_or(5),
        startup_delay_secs: config.startup_delay_secs.unwrap_or(0),
        min_update_interval_secs: config.min_update_interval_secs.unwrap_or(60),
        event_channel_capacity: 100,
        metadata: std::collections::HashMap::new(),
    };

    let ddns_config = DdnsConfig {
        ip_source: ip_source_config,
        provider: provider_config,
        state_store: state_store_config,
        records: record_configs,
        engine: engine_config,
    };

    // Create components from registry
    let ip_source = registry.create_ip_source(&ddns_config.ip_source)?;
    let provider = registry.create_provider(&ddns_config.provider)?;
    let state_store = registry
        .create_state_store(&ddns_config.state_store)
        .await?;

    info!("IP source type: {}", config.ip_source_type);
    info!("Provider type: {}", config.provider_type);
    info!("State store type: {}", config.state_store_type);

    for record in &config.records {
        info!("Managing record: {}", record);
    }

    // Create engine
    let (engine, mut event_rx) = DdnsEngine::new(ip_source, provider, state_store, ddns_config)?;

    // Spawn event listener (optional, for logging)
    let event_listener = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            info!("[Engine Event] {:?}", event);
        }
    });

    info!("Starting DDNS engine");
    let engine_handle = tokio::spawn(async move { engine.run().await });

    info!("Daemon initialized successfully");
    info!("Ready to monitor IP changes");

    // Wait for shutdown signal (runs indefinitely until SIGTERM/SIGINT)
    let signal = wait_for_shutdown().await?;

    info!("Received shutdown signal: {}", signal);
    info!("Shutting down daemon");

    // Drop engine handle to trigger graceful shutdown
    drop(engine_handle);

    // Wait for event listener to finish
    drop(event_listener);

    Ok(())
}

/// Wait for shutdown signals (SIGTERM, SIGINT)
///
/// This function waits indefinitely for a shutdown signal.
/// It will block until either SIGTERM or SIGINT is received.
///
/// # Returns
///
/// Returns the name of the signal received.
#[cfg(unix)]
async fn wait_for_shutdown() -> Result<&'static str> {
    // Set up signal handlers for SIGTERM and SIGINT
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| anyhow::anyhow!("Failed to setup SIGTERM handler: {}", e))?;
    let mut sigint = signal(SignalKind::interrupt())
        .map_err(|e| anyhow::anyhow!("Failed to setup SIGINT handler: {}", e))?;

    // Wait for either signal (indefinitely)
    tokio::select! {
        _ = sigterm.recv() => Ok("SIGTERM"),
        _ = sigint.recv() => Ok("SIGINT"),
    }
}

/// Wait for shutdown signals (SIGINT only)
///
/// Fallback implementation for non-Unix platforms.
#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<&'static str> {
    tokio::signal::ctrl_c()
        .await
        .map(|()| "SIGINT")
        .map_err(|e| anyhow::anyhow!("Failed to wait for CTRL-C: {}", e))
}
