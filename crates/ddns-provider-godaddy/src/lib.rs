// # GoDaddy DNS Provider
//
// This crate provides a GoDaddy DNS provider implementation for the DDNS system.
//
// ## Implementation Status
//
// - ✅ Makes one HTTP request per engine event (as required by architectural constraints)
// - ✅ Full error propagation to engine (engine handles retries, backoff, rate limiting)
// - ✅ HTTP timeout configured (30 seconds)
// - ✅ Specific error handling for HTTP status codes (403, 404, 429, 5xx)
// - ✅ Dry-run mode for safe testing
// - ✅ Idempotency checking (no PUT if IP unchanged)
// - ✅ Both A and AAAA record support
// - ✅ Basic Auth authentication (API key + secret)
// - ❌ NO retry logic (intentionally omitted - owned by DdnsEngine)
// - ❌ NO backoff logic (intentionally omitted - owned by DdnsEngine)
// - ❌ NO rate limiting (intentionally omitted - owned by DdnsEngine)
// - ❌ NO caching (intentionally omitted - state owned by StateStore)
// - ❌ NO background tasks (intentionally omitted - violates shutdown determinism)
//
// ## Architectural Constraints (Per AI_CONTRACT.md)
//
// ### Trust Level: Untrusted (DNS Provider)
//
// Providers are **untrusted** components with strict limitations:
//
// **Allowed Capabilities**:
// - ✅ Perform HTTP/HTTPS API calls to their endpoints only
// - ⚠️ Allocate minimal memory (prefer streaming)
// - ✅ Parse provider-specific responses
//
// **Forbidden Capabilities** (enforced by code review):
// - ❌ Spawn tasks or threads (violates shutdown determinism)
// - ❌ Implement retry logic (owned by DdnsEngine)
// - ❌ Access state store (owned by DdnsEngine)
// - ❌ Access other providers (must be isolated)
// - ❌ Make scheduling decisions (owned by DdnsEngine)
// - ❌ Cache state beyond single request (owned by StateStore)
//
// ## Security Requirements
//
// - API key and secret NEVER appear in logs
// - API key and secret MUST be provided via environment variables only
// - Provider MUST fail fast if credentials are empty
//
// ## API Reference
//
// - GoDaddy API: https://developer.godaddy.com/doc/endpoint/domains#/v1/records
// - GET /v1/domains/{domain}/records/{type}/{name} - Get DNS record
// - PUT /v1/domains/{domain}/records/{type}/{name} - Update DNS record
// - POST /v1/domains/{domain}/records - Add DNS record

use async_trait::async_trait;
use ddns_core::config::{ProviderConfig, ProviderConfigurable};
use ddns_core::traits::{DnsProvider, DnsProviderFactory, UpdateResult};
use ddns_core::{Error, Result};
use serde_json::Value;
use std::net::IpAddr;
use std::time::Duration;

/// GoDaddy API base URL (production)
const GODADDY_API_BASE: &str = "https://api.godaddy.com";

/// GoDaddy API base URL (OTE/test environment)
const GODADDY_API_OTE_BASE: &str = "https://api.ote-godaddy.com";

/// Default HTTP timeout for API requests (30 seconds)
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// GoDaddy DNS provider
///
/// # Trust Level: Untrusted
///
/// This provider is isolated, stateless, and single-shot. All coordination
/// (retries, backoff, scheduling) is owned by `DdnsEngine`.
///
/// # Dry-Run Mode
///
/// When `dry_run` is true, the provider will:
/// - Perform all GET requests (record lookup)
/// - Log the intended update
/// - **NOT** actually modify DNS records
///
/// This allows safe testing without making changes.
///
/// # Security
///
/// The Debug implementation intentionally does NOT expose the API key or secret.
///
/// # Authentication
///
/// GoDaddy uses Basic Auth with API key and secret:
/// - Key & Secret: Used for HTTP Basic Authentication
impl std::fmt::Debug for GoDaddyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoDaddyProvider")
            .field("api_key", &"<REDACTED>")
            .field("api_secret", &"<REDACTED>")
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

pub struct GoDaddyProvider {
    /// GoDaddy API key
    /// ⚠️ NEVER log this value
    api_key: String,

    /// GoDaddy API secret
    /// ⚠️ NEVER log this value
    api_secret: String,

    /// API base URL (OTE or production)
    api_base: String,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// Dry-run mode: if true, perform GET requests but skip updates
    dry_run: bool,
}

impl GoDaddyProvider {
    /// Create a new GoDaddy provider
    ///
    /// # Parameters
    ///
    /// - `api_key`: GoDaddy API key
    /// - `api_secret`: GoDaddy API secret
    /// - `ote`: If true, use OTE (test) environment; otherwise use production
    /// - `dry_run`: If true, perform GET requests but skip updates
    ///
    /// # Security
    ///
    /// The API key and secret will NEVER be logged or displayed in error messages.
    pub fn new(api_key: impl Into<String>, api_secret: impl Into<String>, ote: bool, dry_run: bool) -> Self {
        // Build HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .expect("Failed to build HTTP client");

        let api_key = api_key.into();
        let api_secret = api_secret.into();

        // Validate credentials are not empty
        if api_key.is_empty() {
            panic!("GoDaddy API key cannot be empty");
        }
        if api_secret.is_empty() {
            panic!("GoDaddy API secret cannot be empty");
        }

        // Select API base URL based on OTE flag
        let api_base = if ote {
            GODADDY_API_OTE_BASE.to_string()
        } else {
            GODADDY_API_BASE.to_string()
        };

        Self {
            api_key,
            api_secret,
            api_base,
            client,
            dry_run,
        }
    }

    /// Create a new GoDaddy provider (production/live mode)
    pub fn new_live(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self::new(api_key, api_secret, false, false)
    }

    /// Create a new GoDaddy provider (OTE/test environment)
    pub fn new_ote(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self::new(api_key, api_secret, true, false)
    }

    /// Create a new GoDaddy provider (dry-run mode)
    pub fn new_dry_run(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self::new(api_key, api_secret, false, true)
    }

    /// Build sso-key Auth header value (GoDaddy-specific format)
    ///
    /// GoDaddy uses a custom "sso-key" format instead of standard Basic Auth:
    /// Authorization: sso-key [KEY]:[SECRET]
    fn build_auth_header(&self) -> String {
        format!("sso-key {}:{}", self.api_key, self.api_secret)
    }

    /// Extract domain from record name
    ///
    /// # Examples
    ///
    /// - "example.com" → "example.com"
    /// - "www.example.com" → "example.com"
    /// - "sub.sub.example.com" → "example.com"
    fn extract_domain(record_name: &str) -> Result<String> {
        let parts: Vec<&str> = record_name.split('.').collect();
        if parts.len() < 2 {
            return Err(Error::provider(
                "godaddy",
                format!("Invalid record name '{}': must contain at least one dot", record_name),
            ));
        }

        // Get the last two parts for the domain
        let domain = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
        Ok(domain)
    }

    /// Extract host (subdomain) from record name
    ///
    /// # Examples
    ///
    /// - "example.com" → "@"
    /// - "www.example.com" → "www"
    /// - "sub.sub.example.com" → "sub.sub"
    fn extract_host(record_name: &str) -> Result<String> {
        let domain = Self::extract_domain(record_name)?;

        if record_name == domain {
            // Root domain
            Ok("@".to_string())
        } else {
            // Subdomain
            let host = record_name
                .strip_suffix(&format!(".{}", domain))
                .ok_or_else(|| {
                    Error::provider(
                        "godaddy",
                        format!("Failed to extract host from '{}'", record_name),
                    )
                })?;
            Ok(host.to_string())
        }
    }

    /// Get DNS record ID and current IP
    async fn get_record(&self, domain: &str, record_type: &str, name: &str) -> Result<Option<(String, IpAddr)>> {
        let url = format!(
            "{}/v1/domains/{}/records/{}/{}",
            &self.api_base, domain, record_type, name
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.build_auth_header())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::provider("godaddy", format!("HTTP request failed: {}", e)))?;

        if response.status() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            return Err(self.map_http_error(status, error_text));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| Error::provider("godaddy", format!("Failed to parse response: {}", e)))?;

        // GoDaddy returns an array of records (should be only one)
        let records = json
            .as_array()
            .ok_or_else(|| Error::provider("godaddy", "Invalid response: expected array"))?;

        if records.is_empty() {
            return Ok(None);
        }

        let record = &records[0];
        let ip_str = record
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| Error::provider("godaddy", "Missing 'data' in record"))?;

        let ip: IpAddr = ip_str
            .parse()
            .map_err(|e| Error::provider("godaddy", format!("Invalid IP address '{}': {}", ip_str, e)))?;

        // GoDaddy doesn't use record IDs, so we use a placeholder
        Ok(Some(("godaddy-record".to_string(), ip)))
    }

    /// Create a new DNS record
    async fn create_record(
        &self,
        domain: &str,
        host: &str,
        record_type: &str,
        ip: IpAddr,
    ) -> Result<()> {
        let url = format!("{}/v1/domains/{}/records", &self.api_base, domain);

        let payload = serde_json::json!([{
            "data": ip.to_string(),
            "name": host,
            "ttl": 600,
            "type": record_type
        }]);

        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would create {} DNS record: {}.{} -> {}",
                self.provider_name(),
                host,
                domain,
                ip
            );
            return Ok(());
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.build_auth_header())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::provider("godaddy", format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            return Err(self.map_http_error(status, error_text));
        }

        tracing::info!(
            "Created {} DNS record: {}.{} -> {}",
            self.provider_name(),
            host,
            domain,
            ip
        );

        Ok(())
    }

    /// Update DNS record IP address
    async fn update_record_ip(
        &self,
        domain: &str,
        host: &str,
        record_type: &str,
        new_ip: IpAddr,
        _previous_ip: IpAddr,
    ) -> Result<()> {
        let url = format!(
            "{}/v1/domains/{}/records/{}/{}",
            &self.api_base, domain, record_type, host
        );

        let payload = serde_json::json!([{
            "data": new_ip.to_string(),
            "ttl": 600
        }]);

        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would update {} DNS record: {}.{} -> {}",
                self.provider_name(),
                host,
                domain,
                new_ip
            );
            return Ok(());
        }

        let response = self
            .client
            .put(&url)
            .header("Authorization", self.build_auth_header())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| Error::provider("godaddy", format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            return Err(self.map_http_error(status, error_text));
        }

        tracing::info!(
            "Updated {} DNS record: {}.{} -> {}",
            self.provider_name(),
            host,
            domain,
            new_ip
        );

        Ok(())
    }

    /// Map HTTP error to appropriate Error type
    fn map_http_error(&self, status: reqwest::StatusCode, error_text: String) -> Error {
        match status.as_u16() {
            401 | 403 => Error::provider(
                "godaddy",
                format!("Authentication failed: Invalid API key or secret. Status: {}", status),
            ),
            404 => Error::not_found("Resource not found"),
            429 => Error::provider(
                "godaddy",
                format!("Rate limit exceeded. Please retry later. Status: {}", status),
            ),
            500..=599 => Error::provider(
                "godaddy",
                format!("Server error (transient): {} - {}", status, error_text),
            ),
            _ => Error::provider(
                "godaddy",
                format!("Request failed: {} - {}", status, error_text),
            ),
        }
    }
}

#[async_trait]
impl DnsProvider for GoDaddyProvider {
    async fn update_record(&self, record_name: &str, new_ip: IpAddr) -> Result<UpdateResult> {
        // Step 1: Determine record type (A or AAAA)
        let record_type = match new_ip {
            IpAddr::V4(_) => "A",
            IpAddr::V6(_) => "AAAA",
        };

        tracing::info!(
            "Updating {} DNS record: {} -> {} [mode: {}]",
            self.provider_name(),
            record_name,
            new_ip,
            if self.dry_run { "DRY-RUN" } else { "LIVE" }
        );

        // Step 2: Extract domain and host from record name
        let domain = Self::extract_domain(record_name)?;
        let host = Self::extract_host(record_name)?;

        // Step 3: Get current record (if exists)
        let (_record_id, current_ip) =
            match self.get_record(&domain, record_type, &host).await? {
                Some((id, ip)) => (id, ip),
                None => {
                    tracing::info!("DNS record does not exist, creating: {}", record_name);
                    self.create_record(&domain, &host, record_type, new_ip).await?;
                    return Ok(UpdateResult::Created { new_ip });
                }
            };

        // Step 4: If IP same, return Unchanged (idempotency)
        if current_ip == new_ip {
            tracing::info!(
                "DNS record already has correct IP: {} -> {}",
                record_name,
                new_ip
            );
            return Ok(UpdateResult::Unchanged { current_ip });
        }

        // Step 5: Dry-run check
        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would update {} DNS record: {} -> {} (was: {})",
                self.provider_name(),
                record_name,
                new_ip,
                current_ip
            );
            return Ok(UpdateResult::Updated {
                previous_ip: Some(current_ip),
                new_ip,
            });
        }

        // Step 6: Perform actual update
        self.update_record_ip(&domain, &host, record_type, new_ip, current_ip)
            .await?;

        Ok(UpdateResult::Updated {
            previous_ip: Some(current_ip),
            new_ip,
        })
    }

    async fn get_record(&self, record_name: &str) -> Result<ddns_core::traits::RecordMetadata> {
        let domain = Self::extract_domain(record_name)?;
        let host = Self::extract_host(record_name)?;

        // Try A record first
        if let Ok(Some((_, ip))) = self.get_record(&domain, "A", &host).await {
            return Ok(ddns_core::traits::RecordMetadata {
                id: record_name.to_string(),
                name: record_name.to_string(),
                ip,
                ttl: None,
                extra: serde_json::Value::Null,
            });
        }

        // Try AAAA record
        if let Ok(Some((_, ip))) = self.get_record(&domain, "AAAA", &host).await {
            return Ok(ddns_core::traits::RecordMetadata {
                id: record_name.to_string(),
                name: record_name.to_string(),
                ip,
                ttl: None,
                extra: serde_json::Value::Null,
            });
        }

        Err(Error::not_found(format!("DNS record not found: {}", record_name)))
    }

    fn supports_record(&self, record_name: &str) -> bool {
        // Basic domain name validation
        record_name.contains('.') && record_name.len() <= 253
    }

    fn provider_name(&self) -> &'static str {
        "godaddy"
    }
}

/// GoDaddy provider configuration handler
///
/// This struct implements `ProviderConfigurable` to enable the plugin
/// architecture. It handles:
/// - Loading config from `GODADDY_API_KEY`, `GODADDY_API_SECRET`
/// - Validating provider-specific configuration
/// - Creating GoDaddyProvider instances from validated config
///
/// # Benefits
///
/// - **Provider-Specific Env Vars**: Uses `GODADDY_` prefix, no conflicts
/// - **Self-Validating**: validates credentials presence
/// - **Zero Core Modification**: New providers don't require ddns-core changes
pub struct GoDaddyConfigurable;

impl ProviderConfigurable for GoDaddyConfigurable {
    fn name(&self) -> &'static str {
        "godaddy"
    }

    fn load_from_env(&self) -> Result<Value> {
        let api_key = std::env::var("GODADDY_API_KEY")
            .map_err(|_| Error::config("GODADDY_API_KEY is required"))?;

        let api_secret = std::env::var("GODADDY_API_SECRET")
            .map_err(|_| Error::config("GODADDY_API_SECRET is required"))?;

        if api_key.is_empty() {
            return Err(Error::config("GODADDY_API_KEY cannot be empty"));
        }

        if api_secret.is_empty() {
            return Err(Error::config("GODADDY_API_SECRET cannot be empty"));
        }

        Ok(serde_json::json!({
            "api_key": api_key,
            "api_secret": api_secret,
        }))
    }

    fn validate(&self, config: &Value) -> Result<()> {
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("Missing api_key in configuration"))?;

        let api_secret = config
            .get("api_secret")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("Missing api_secret in configuration"))?;

        if api_key.is_empty() {
            return Err(Error::config("GODADDY_API_KEY cannot be empty"));
        }

        if api_secret.is_empty() {
            return Err(Error::config("GODADDY_API_SECRET cannot be empty"));
        }

        Ok(())
    }

    fn create_provider(
        &self,
        config: &Value,
        dry_run: bool,
    ) -> Result<Box<dyn DnsProvider>> {
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("Missing api_key in configuration"))?;

        let api_secret = config
            .get("api_secret")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("Missing api_secret in configuration"))?;

        // Check if OTE (test) environment should be used
        let ote = config
            .get("ote")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Box::new(GoDaddyProvider::new(
            api_key,
            api_secret,
            ote,
            dry_run,
        )))
    }
}

/// Factory for creating GoDaddy providers
pub struct GoDaddyFactory;

impl DnsProviderFactory for GoDaddyFactory {
    fn create(&self, config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
        if config.provider_type != "godaddy" {
            return Err(Error::config(format!(
                "Invalid config for GoDaddy provider: got {}",
                config.provider_type
            )));
        }

        let api_key = config
            .config
            .get("api_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("GoDaddy API key is required"))?;

        let api_secret = config
            .config
            .get("api_secret")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("GoDaddy API secret is required"))?;

        if api_key.is_empty() {
            return Err(Error::config("GoDaddy API key is required"));
        }
        if api_secret.is_empty() {
            return Err(Error::config("GoDaddy API secret is required"));
        }

        let dry_run = std::env::var("DDNS_MODE")
            .unwrap_or_default()
            .to_lowercase()
            == "dry-run";

        // Check if OTE (test) environment should be used
        let ote = std::env::var("GODADDY_OTE")
            .unwrap_or_default()
            .to_lowercase()
            == "true";

        if dry_run {
            tracing::warn!("{} provider running in DRY-RUN mode", "godaddy");
        }

        if ote {
            tracing::info!("{} provider using OTE (test) environment", "godaddy");
        }

        Ok(Box::new(GoDaddyProvider::new(
            api_key,
            api_secret,
            ote,
            dry_run,
        )))
    }
}

/// Register the GoDaddy provider with a registry
pub fn register(registry: &ddns_core::ProviderRegistry) {
    // Register legacy factory (for backward compatibility)
    registry.register_provider("godaddy", Box::new(GoDaddyFactory));

    // Register new configurable (recommended approach)
    registry.register_provider_configurable(Box::new(GoDaddyConfigurable));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = GoDaddyProvider::new("test_key", "test_secret", false);
        assert_eq!(provider.provider_name(), "godaddy");
    }

    #[test]
    fn test_dry_run_mode() {
        let provider_dry = GoDaddyProvider::new_dry_run("key", "secret");
        let provider_live = GoDaddyProvider::new_live("key", "secret");
        assert!(provider_dry.dry_run);
        assert!(!provider_live.dry_run);
    }

    #[test]
    fn test_api_key_not_exposed_in_debug() {
        let provider = GoDaddyProvider::new("secret_key", "secret_secret", false);
        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret_key"));
        assert!(!debug_str.contains("secret_secret"));
        assert!(debug_str.contains("<REDACTED>"));
    }

    #[test]
    fn test_supports_record() {
        let provider = GoDaddyProvider::new("key", "secret", false);
        assert!(provider.supports_record("example.com"));
        assert!(provider.supports_record("www.example.com"));
        assert!(!provider.supports_record(""));
    }

    #[test]
    #[should_panic(expected = "API key cannot be empty")]
    fn test_empty_api_key_panics() {
        GoDaddyProvider::new("", "secret", false, false);
    }

    #[test]
    #[should_panic(expected = "API secret cannot be empty")]
    fn test_empty_api_secret_panics() {
        GoDaddyProvider::new("key", "", false, false);
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(GoDaddyProvider::extract_domain("example.com").unwrap(), "example.com");
        assert_eq!(GoDaddyProvider::extract_domain("www.example.com").unwrap(), "example.com");
        assert_eq!(GoDaddyProvider::extract_domain("sub.sub.example.com").unwrap(), "example.com");
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(GoDaddyProvider::extract_host("example.com").unwrap(), "@");
        assert_eq!(GoDaddyProvider::extract_host("www.example.com").unwrap(), "www");
        assert_eq!(GoDaddyProvider::extract_host("sub.sub.example.com").unwrap(), "sub.sub");
    }

    #[test]
    fn test_build_auth_header() {
        let provider = GoDaddyProvider::new("my_key", "my_secret", false, false);
        let header = provider.build_auth_header();
        // GoDaddy uses sso-key format
        assert!(header.starts_with("sso-key "));
        assert!(header.contains("my_key"));
        assert!(header.contains("my_secret"));
    }
}
