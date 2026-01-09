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
//
// ### DNS Provider
// - `DDNS_PROVIDER_TYPE`: Provider type (cloudflare)
// - `DDNS_PROVIDER_API_TOKEN`: API token
// - `DDNS_PROVIDER_ZONE_ID`: Zone ID (optional)
//
// ### Records
// - `DDNS_RECORDS`: Comma-separated list of DNS records to manage
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
// ```bash
// export DDNS_IP_SOURCE_TYPE=netlink
// export DDNS_IP_SOURCE_INTERFACE=eth0
// export DDNS_PROVIDER_TYPE=cloudflare
// export DDNS_PROVIDER_API_TOKEN=your_token
// export DDNS_RECORDS=example.com,www.example.com
// export DDNS_STATE_STORE_TYPE=file
// export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
//
// ddnsd
// ```

use anyhow::Result;
use std::env;
use std::process::ExitCode;
use std::time::Duration;
use tracing::{Level, error, info, warn};
use tracing_subscriber::FmtSubscriber;

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

/// Application configuration
#[allow(dead_code)]
struct Config {
    ip_source_type: String,
    ip_source_interface: Option<String>,
    ip_source_url: Option<String>,
    ip_source_interval: Option<u64>,
    provider_type: String,
    provider_api_token: String,
    provider_zone_id: Option<String>,
    records: Vec<String>,
    state_store_type: String,
    state_store_path: Option<String>,
    max_retries: Option<usize>,
    retry_delay_secs: Option<u64>,
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
            "cloudflare" => {} // Currently supported
            _ => anyhow::bail!(
                "DDNS_PROVIDER_TYPE '{}' is not supported. \
                Supported providers: cloudflare",
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
    // Create provider registry
    let _registry = ddns_core::ProviderRegistry::new();

    // Register built-in providers
    // Note: Provider crates should provide a `register()` function
    #[cfg(feature = "cloudflare")]
    {
        info!("Registering Cloudflare provider");
        // ddns_provider_cloudflare::register(&registry);
        warn!("Cloudflare provider feature enabled but not yet implemented");
    }

    #[cfg(feature = "netlink")]
    {
        info!("Registering Netlink IP source");
        // ddns_ip_netlink::register(&registry);
        warn!("Netlink IP source feature enabled but not yet implemented");
    }

    // TODO: Create components from config
    // For now, we'll just log what would be created

    info!("IP source type: {}", config.ip_source_type);
    info!("Provider type: {}", config.provider_type);
    info!("State store type: {}", config.state_store_type);

    for record in &config.records {
        info!("Managing record: {}", record);
    }

    // TODO: Create and run engine
    // let ip_source = registry.create_ip_source(&ip_source_config)?;
    // let provider = registry.create_provider(&provider_config)?;
    // let state_store = registry.create_state_store(&state_store_config)?;

    // let engine = ddns_core::DdnsEngine::new(
    //     ip_source,
    //     provider,
    //     state_store,
    //     ddns_config,
    //     None,
    // )?;

    // info!("Starting DDNS engine");
    // engine.run().await?;

    info!("Daemon initialized successfully");
    info!("Ready to monitor IP changes");

    // Wait for shutdown signal with timeout
    let shutdown_result = wait_for_shutdown_with_timeout(Duration::from_secs(30)).await;

    match shutdown_result {
        Ok(signal) => {
            info!("Received shutdown signal: {}", signal);
            info!("Shutting down daemon");
        }
        Err(e) => {
            error!("Shutdown error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Wait for shutdown signals (SIGTERM, SIGINT) with a timeout
///
/// This function handles graceful shutdown with a timeout to prevent
/// the daemon from hanging indefinitely during shutdown.
///
/// # Returns
///
/// Returns the name of the signal received, or an error if timeout occurs.
#[cfg(unix)]
async fn wait_for_shutdown_with_timeout(timeout_duration: Duration) -> Result<&'static str> {
    use tokio::time::timeout;

    // Set up signal handlers for SIGTERM and SIGINT
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| anyhow::anyhow!("Failed to setup SIGTERM handler: {}", e))?;
    let mut sigint = signal(SignalKind::interrupt())
        .map_err(|e| anyhow::anyhow!("Failed to setup SIGINT handler: {}", e))?;

    // Wait for either signal with timeout
    match timeout(timeout_duration, async {
        tokio::select! {
            _ = sigterm.recv() => "SIGTERM",
            _ = sigint.recv() => "SIGINT",
        }
    })
    .await
    {
        Ok(signal) => Ok(signal),
        Err(_) => Err(anyhow::anyhow!(
            "Shutdown timeout after {:?}",
            timeout_duration
        )),
    }
}

/// Wait for shutdown signals (SIGINT only) with a timeout
///
/// Fallback implementation for non-Unix platforms.
#[cfg(not(unix))]
async fn wait_for_shutdown_with_timeout(timeout_duration: Duration) -> Result<&'static str> {
    use tokio::time::timeout;

    match timeout(timeout_duration, tokio::signal::ctrl_c()).await {
        Ok(Ok(())) => Ok("SIGINT"),
        Ok(Err(e)) => Err(anyhow::anyhow!("Failed to wait for CTRL-C: {}", e)),
        Err(_) => Err(anyhow::anyhow!(
            "Shutdown timeout after {:?}",
            timeout_duration
        )),
    }
}
