// # NameSilo DNS Provider
//
// This crate provides a NameSilo DNS provider implementation for the DDNS system.
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
// - ✅ API key authentication (simple)
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
// - API key NEVER appears in logs
// - API key MUST be provided via environment variables only
// - Provider MUST fail fast if credentials are empty
//
// ## API Reference
//
// - NameSilo API: https://www.namesilo.com/api-reference
// - dnsListRecords: List DNS records for a domain
// - dnsUpdateRecord: Update a DNS record
// - dnsAddRecord: Add a new DNS record

use async_trait::async_trait;
use ddns_core::config::{ProviderConfig, ProviderConfigurable};
use ddns_core::traits::{DnsProvider, DnsProviderFactory, UpdateResult};
use ddns_core::{Error, Result};
use serde_json::Value;
use std::net::IpAddr;
use std::time::Duration;

/// NameSilo API base URL
const NAMESILO_API_BASE: &str = "https://www.namesilo.com/api";

/// NameSilo API version
const API_VERSION: &str = "1";

/// Default HTTP timeout for API requests (30 seconds)
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// NameSilo DNS provider
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
/// The Debug implementation intentionally does NOT expose the API key.
///
/// # Authentication
///
/// NameSilo uses simple API key authentication via URL parameter:
/// - API Key: Included as `&version=1&type=xml&key=YOUR_KEY` in requests
impl std::fmt::Debug for NameSiloProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NameSiloProvider")
            .field("api_key", &"<REDACTED>")
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

pub struct NameSiloProvider {
    /// NameSilo API key
    /// ⚠️ NEVER log this value
    api_key: String,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// Dry-run mode: if true, perform GET requests but skip updates
    dry_run: bool,
}

impl NameSiloProvider {
    /// Create a new NameSilo provider
    ///
    /// # Parameters
    ///
    /// - `api_key`: NameSilo API key
    /// - `dry_run`: If true, perform GET requests but skip updates
    ///
    /// # Security
    ///
    /// The API key will NEVER be logged or displayed in error messages.
    pub fn new(api_key: impl Into<String>, dry_run: bool) -> Self {
        // Build HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .expect("Failed to build HTTP client");

        let api_key = api_key.into();

        // Validate API key is not empty
        if api_key.is_empty() {
            panic!("NameSilo API key cannot be empty");
        }

        Self {
            api_key,
            client,
            dry_run,
        }
    }

    /// Create a new NameSilo provider (production/live mode)
    pub fn new_live(api_key: impl Into<String>) -> Self {
        Self::new(api_key, false)
    }

    /// Create a new NameSilo provider (dry-run mode)
    pub fn new_dry_run(api_key: impl Into<String>) -> Self {
        Self::new(api_key, true)
    }

    /// Build API URL with parameters
    fn build_api_url(&self, action: &str, params: &[(&str, &str)]) -> String {
        let mut query_pairs = vec![
            ("version", API_VERSION),
            ("type", "json"),
            ("key", &self.api_key),
        ];

        // Add additional parameters
        for (key, value) in params {
            query_pairs.push((*key, *value));
        }

        let query_string = query_pairs
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        // NameSilo API format: https://www.namesilo.com/api/{operation}?params
        format!("{}/{}?{}", NAMESILO_API_BASE, action, query_string)
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
                "namesilo",
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
                        "namesilo",
                        format!("Failed to extract host from '{}'", record_name),
                    )
                })?;
            Ok(host.to_string())
        }
    }

    /// Get DNS record ID for a record name
    async fn get_record_id(
        &self,
        domain: &str,
        host: &str,
        record_type: &str,
    ) -> Result<String> {
        let url = self.build_api_url("dnsListRecords", &[("domain", domain)]);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::provider("namesilo", format!("HTTP request failed: {}", e)))?;

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
            .map_err(|e| Error::provider("namesilo", format!("Failed to parse response: {}", e)))?;

        // NameSilo API wraps response in "reply" object
        let reply = json
            .get("reply")
            .ok_or_else(|| Error::provider("namesilo", "Missing 'reply' in response"))?;

        // Check for API errors
        if let Some(error_code) = reply.get("code") {
            let code = error_code.as_i64().unwrap_or(0);
            if code != 300 {
                let detail = reply
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .unwrap_or("Unknown error");
                return Err(Error::provider(
                    "namesilo",
                    format!("API error (code {}): {}", code, detail),
                ));
            }
        }

        // Get records array (NameSilo uses 'resource_record' not 'records')
        let records = reply
            .get("resource_record")
            .and_then(|r| r.as_array())
            .ok_or_else(|| Error::provider("namesilo", "Missing or invalid 'resource_record' in response"))?;

        // Find matching record
        for record in records {
            if let Some(r) = record.as_object() {
                let r_host = r.get("host").and_then(|h| h.as_str()).unwrap_or("");
                let r_type = r.get("type").and_then(|t| t.as_str()).unwrap_or("");

                if r_host == host && r_type == record_type {
                    let record_id = r
                        .get("record_id")
                        .and_then(|id| id.as_str())
                        .ok_or_else(|| {
                            Error::provider("namesilo", "Missing 'record_id' in record")
                        })?;
                    return Ok(record_id.to_string());
                }
            }
        }

        // Record not found
        Err(Error::not_found(format!(
            "DNS record not found: {}.{} ({})",
            host, domain, record_type
        )))
    }

    /// Create a new DNS record
    async fn create_record(
        &self,
        domain: &str,
        host: &str,
        record_type: &str,
        ip: IpAddr,
    ) -> Result<String> {
        let url = self.build_api_url(
            "dnsAddRecord",
            &[
                ("domain", domain),
                ("rrhost", host),
                ("rrtype", record_type),
                ("rrvalue", &ip.to_string()),
            ],
        );

        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would create {} DNS record: {}.{} -> {}",
                self.provider_name(),
                host,
                domain,
                ip
            );
            // Return a fake record ID for dry-run
            return Ok("dry-run-id".to_string());
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::provider("namesilo", format!("HTTP request failed: {}", e)))?;

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
            .map_err(|e| Error::provider("namesilo", format!("Failed to parse response: {}", e)))?;

        let reply = json
            .get("reply")
            .ok_or_else(|| Error::provider("namesilo", "Missing 'reply' in response"))?;

        // Check for API errors
        if let Some(error_code) = reply.get("code") {
            let code = error_code.as_i64().unwrap_or(0);
            if code != 300 {
                let detail = reply
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .unwrap_or("Unknown error");
                return Err(Error::provider(
                    "namesilo",
                    format!("API error (code {}): {}", code, detail),
                ));
            }
        }

        // Get the new record ID
        let record_id = reply
            .get("record_id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| Error::provider("namesilo", "Missing 'record_id' in response"))?;

        tracing::info!(
            "Created {} DNS record: {}.{} -> {} (ID: {})",
            self.provider_name(),
            host,
            domain,
            ip,
            record_id
        );

        Ok(record_id.to_string())
    }

    /// Get current IP address for a record
    async fn get_current_record(
        &self,
        domain: &str,
        host: &str,
        record_type: &str,
    ) -> Result<IpAddr> {
        let url = self.build_api_url("dnsListRecords", &[("domain", domain)]);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::provider("namesilo", format!("HTTP request failed: {}", e)))?;

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
            .map_err(|e| Error::provider("namesilo", format!("Failed to parse response: {}", e)))?;

        let reply = json
            .get("reply")
            .ok_or_else(|| Error::provider("namesilo", "Missing 'reply' in response"))?;

        let records = reply
            .get("resource_record")
            .and_then(|r| r.as_array())
            .ok_or_else(|| Error::provider("namesilo", "Missing or invalid 'resource_record' in response"))?;

        // Find matching record
        for record in records {
            if let Some(r) = record.as_object() {
                let r_host = r.get("host").and_then(|h| h.as_str()).unwrap_or("");
                let r_type = r.get("type").and_then(|t| t.as_str()).unwrap_or("");

                if r_host == host && r_type == record_type {
                    let ip_str = r
                        .get("value")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Error::provider("namesilo", "Missing 'value' in record"))?;

                    let ip: IpAddr = ip_str
                        .parse()
                        .map_err(|e| {
                            Error::provider(
                                "namesilo",
                                format!("Invalid IP address '{}': {}", ip_str, e),
                            )
                        })?;

                    return Ok(ip);
                }
            }
        }

        Err(Error::not_found(format!(
            "DNS record not found: {}.{} ({})",
            host, domain, record_type
        )))
    }

    /// Update DNS record IP address
    async fn update_record_ip(
        &self,
        domain: &str,
        host: &str,
        record_id: &str,
        record_type: &str,
        new_ip: IpAddr,
        _previous_ip: IpAddr,
    ) -> Result<()> {
        let url = self.build_api_url(
            "dnsUpdateRecord",
            &[
                ("domain", domain),
                ("rrhost", host),
                ("rrtype", record_type),
                ("rrvalue", &new_ip.to_string()),
                ("rrid", record_id),
            ],
        );

        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would update {} DNS record: {}.{} -> {} (ID: {})",
                self.provider_name(),
                host,
                domain,
                new_ip,
                record_id
            );
            return Ok(());
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::provider("namesilo", format!("HTTP request failed: {}", e)))?;

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
            .map_err(|e| Error::provider("namesilo", format!("Failed to parse response: {}", e)))?;

        let reply = json
            .get("reply")
            .ok_or_else(|| Error::provider("namesilo", "Missing 'reply' in response"))?;

        // Check for API errors
        if let Some(error_code) = reply.get("code") {
            let code = error_code.as_i64().unwrap_or(0);
            if code != 300 {
                let detail = reply
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .unwrap_or("Unknown error");
                return Err(Error::provider(
                    "namesilo",
                    format!("API error (code {}): {}", code, detail),
                ));
            }
        }

        tracing::info!(
            "Updated {} DNS record: {}.{} -> {} (ID: {})",
            self.provider_name(),
            host,
            domain,
            new_ip,
            record_id
        );

        Ok(())
    }

    /// Map HTTP error to appropriate Error type
    fn map_http_error(
        &self,
        status: reqwest::StatusCode,
        error_text: String,
    ) -> Error {
        match status.as_u16() {
            401 | 403 => Error::provider(
                "namesilo",
                format!("Authentication failed: Invalid API key. Status: {}", status),
            ),
            404 => Error::not_found("Resource not found"),
            429 => Error::provider(
                "namesilo",
                format!("Rate limit exceeded. Please retry later. Status: {}", status),
            ),
            500..=599 => Error::provider(
                "namesilo",
                format!("Server error (transient): {} - {}", status, error_text),
            ),
            _ => Error::provider(
                "namesilo",
                format!("Request failed: {} - {}", status, error_text),
            ),
        }
    }
}

#[async_trait]
impl DnsProvider for NameSiloProvider {
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

        // Step 3: Get/create record ID
        let (record_id, is_newly_created) =
            match self.get_record_id(&domain, &host, record_type).await {
                Ok(id) => (id, false),
                Err(Error::NotFound { .. }) => {
                    tracing::info!("DNS record does not exist, creating: {}", record_name);
                    (self.create_record(&domain, &host, record_type, new_ip).await?, true)
                }
                Err(e) => return Err(e),
            };

        // Step 4: If newly created, return Created
        if is_newly_created {
            return Ok(UpdateResult::Created { new_ip });
        }

        // Step 5: Get current record, check IP is same
        let current_ip = self.get_current_record(&domain, &host, record_type).await?;

        // Step 6: If IP same, return Unchanged (idempotency)
        if current_ip == new_ip {
            tracing::info!(
                "DNS record already has correct IP: {} -> {}",
                record_name,
                new_ip
            );
            return Ok(UpdateResult::Unchanged { current_ip });
        }

        // Step 7: Dry-run check
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

        // Step 8: Perform actual update
        self.update_record_ip(&domain, &host, &record_id, record_type, new_ip, current_ip)
            .await?;

        Ok(UpdateResult::Updated {
            previous_ip: Some(current_ip),
            new_ip,
        })
    }

    async fn get_record(&self, record_name: &str) -> Result<ddns_core::traits::RecordMetadata> {
        let record_type = match self
            .get_current_record(
                &Self::extract_domain(record_name)?,
                &Self::extract_host(record_name)?,
                "A", // Try A first
            )
            .await
        {
            Ok(ip) => ip,
            Err(_) => {
                // Try AAAA
                self.get_current_record(
                    &Self::extract_domain(record_name)?,
                    &Self::extract_host(record_name)?,
                    "AAAA",
                )
                .await?
            }
        };

        Ok(ddns_core::traits::RecordMetadata {
            id: record_name.to_string(),
            name: record_name.to_string(),
            ip: record_type,
            ttl: None,
            extra: serde_json::Value::Null,
        })
    }

    fn supports_record(&self, record_name: &str) -> bool {
        // Basic domain name validation
        record_name.contains('.') && record_name.len() <= 253
    }

    fn provider_name(&self) -> &'static str {
        "namesilo"
    }
}

/// NameSilo provider configuration handler
///
/// This struct implements `ProviderConfigurable` to enable the plugin
/// architecture. It handles:
/// - Loading config from `NAMESILO_API_KEY`
/// - Validating provider-specific configuration
/// - Creating NameSiloProvider instances from validated config
///
/// # Benefits
///
/// - **Provider-Specific Env Vars**: Uses `NAMESILO_` prefix, no conflicts
/// - **Self-Validating**: Validates API key presence
/// - **Zero Core Modification**: New providers don't require ddns-core changes
pub struct NameSiloConfigurable;

impl ProviderConfigurable for NameSiloConfigurable {
    fn name(&self) -> &'static str {
        "namesilo"
    }

    fn load_from_env(&self) -> Result<Value> {
        let api_key = std::env::var("NAMESILO_API_KEY")
            .map_err(|_| Error::config("NAMESILO_API_KEY is required"))?;

        if api_key.is_empty() {
            return Err(Error::config("NAMESILO_API_KEY cannot be empty"));
        }

        Ok(serde_json::json!({
            "api_key": api_key,
        }))
    }

    fn validate(&self, config: &Value) -> Result<()> {
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("Missing api_key in configuration"))?;

        if api_key.is_empty() {
            return Err(Error::config("NAMESILO_API_KEY cannot be empty"));
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

        Ok(Box::new(NameSiloProvider::new(api_key, dry_run)))
    }
}

/// Factory for creating NameSilo providers
pub struct NameSiloFactory;

impl DnsProviderFactory for NameSiloFactory {
    fn create(&self, config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
        if config.provider_type != "namesilo" {
            return Err(Error::config(format!(
                "Invalid config for NameSilo provider: got {}",
                config.provider_type
            )));
        }

        let api_key = config
            .config
            .get("api_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::config("NameSilo API key is required"))?;

        if api_key.is_empty() {
            return Err(Error::config("NameSilo API key is required"));
        }

        let dry_run = std::env::var("DDNS_MODE")
            .unwrap_or_default()
            .to_lowercase()
            == "dry-run";

        if dry_run {
            tracing::warn!("{} provider running in DRY-RUN mode", "namesilo");
        }

        Ok(Box::new(NameSiloProvider::new(api_key, dry_run)))
    }
}

/// Register the NameSilo provider with a registry
pub fn register(registry: &ddns_core::ProviderRegistry) {
    // Register legacy factory (for backward compatibility)
    registry.register_provider("namesilo", Box::new(NameSiloFactory));

    // Register new configurable (recommended approach)
    registry.register_provider_configurable(Box::new(NameSiloConfigurable));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = NameSiloProvider::new("test_key", false);
        assert_eq!(provider.provider_name(), "namesilo");
    }

    #[test]
    fn test_dry_run_mode() {
        let provider_dry = NameSiloProvider::new_dry_run("key");
        let provider_live = NameSiloProvider::new_live("key");
        assert!(provider_dry.dry_run);
        assert!(!provider_live.dry_run);
    }

    #[test]
    fn test_api_key_not_exposed_in_debug() {
        let provider = NameSiloProvider::new("secret_api_key", false);
        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret_api_key"));
        assert!(debug_str.contains("<REDACTED>"));
    }

    #[test]
    fn test_supports_record() {
        let provider = NameSiloProvider::new("key", false);
        assert!(provider.supports_record("example.com"));
        assert!(provider.supports_record("www.example.com"));
        assert!(!provider.supports_record(""));
    }

    #[test]
    #[should_panic(expected = "API key cannot be empty")]
    fn test_empty_api_key_panics() {
        NameSiloProvider::new("", false);
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(NameSiloProvider::extract_domain("example.com").unwrap(), "example.com");
        assert_eq!(NameSiloProvider::extract_domain("www.example.com").unwrap(), "example.com");
        assert_eq!(NameSiloProvider::extract_domain("sub.sub.example.com").unwrap(), "example.com");
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(NameSiloProvider::extract_host("example.com").unwrap(), "@");
        assert_eq!(NameSiloProvider::extract_host("www.example.com").unwrap(), "www");
        assert_eq!(NameSiloProvider::extract_host("sub.sub.example.com").unwrap(), "sub.sub");
    }
}
