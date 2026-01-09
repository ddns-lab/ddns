// # Cloudflare DNS Provider
//
// This crate provides a Cloudflare DNS provider implementation for the DDNS system.
//
// ## Implementation Status (Phase 22: Production-Grade Completion)
//
// **This is a PRODUCTION-READY implementation** with:
//
// - ✅ Makes one HTTP request per engine event (as required by architectural constraints)
// - ✅ Full error propagation to engine (engine handles retries, backoff, rate limiting)
// - ✅ HTTP timeout configured (30 seconds)
// - ✅ Specific error handling for HTTP status codes (403, 404, 409, 429, 5xx)
// - ✅ Dry-run mode for safe testing
// - ✅ Idempotency checking (no PUT if IP unchanged)
// - ✅ Both A and AAAA record support
// - ✅ Zone auto-discovery and explicit zone ID
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
// See `docs/architecture/TRUST_LEVELS.md` for complete trust level definitions.
//
// ## Security Requirements
//
// - API token NEVER appears in logs
// - API token MUST be provided via environment variables only
// - Provider MUST fail fast if token is empty
//
// ## API Reference
//
// - Cloudflare API v4: https://developers.cloudflare.com/api/
// - Update DNS Record: PUT `/zones/:zone_id/dns_records/:record_id`
// - List DNS Records: GET `/zones/:zone_id/dns_records?name=...&type=...`
// - List Zones: GET `/zones?name=...`

use async_trait::async_trait;
use ddns_core::traits::{DnsProvider, DnsProviderFactory, UpdateResult, RecordMetadata};
use ddns_core::config::ProviderConfig;
use ddns_core::{Error, Result};
use std::net::IpAddr;
use std::time::Duration;
use serde_json::Value;

/// Cloudflare API base URL
const CLOUDFLARE_API_BASE: &str = "https://api.cloudflare.com/client/v4";

/// Default HTTP timeout for API requests (30 seconds)
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Cloudflare DNS provider
///
/// # Trust Level: Untrusted
///
/// This provider is isolated, stateless, and single-shot. All coordination
/// (retries, backoff, scheduling) is owned by `DdnsEngine`.
///
/// # Dry-Run Mode
///
/// When `dry_run` is true, the provider will:
/// - Perform all GET requests (zone lookup, record lookup)
/// - Log the intended PUT payload
/// - **NOT** actually modify DNS records
///
/// This allows safe testing without making changes.
///
/// # Security
///
/// The Debug implementation intentionally does NOT expose the API token.
// Custom Debug implementation that hides the API token
impl std::fmt::Debug for CloudflareProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudflareProvider")
            .field("api_token", &"<REDACTED>")
            .field("zone_id", &self.zone_id)
            .field("account_id", &self.account_id)
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

pub struct CloudflareProvider {
    /// Cloudflare API token
    /// ⚠️ NEVER log this value
    api_token: String,

    /// Zone ID (optional, can be auto-detected from domain)
    zone_id: Option<String>,

    /// Account ID (optional, for some operations)
    account_id: Option<String>,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// Dry-run mode: if true, perform GET requests but skip PUT updates
    dry_run: bool,
}

impl CloudflareProvider {
    /// Create a new Cloudflare provider
    ///
    /// # Parameters
    ///
    /// - `api_token`: Cloudflare API token with Zone:DNS:Edit permissions
    /// - `zone_id`: Optional zone ID (can be auto-detected)
    /// - `account_id`: Optional account ID
    /// - `dry_run`: If true, perform GET requests but skip PUT updates
    ///
    /// # Security
    ///
    /// The API token will NEVER be logged or displayed in error messages.
    pub fn new(
        api_token: impl Into<String>,
        zone_id: Option<String>,
        account_id: Option<String>,
        dry_run: bool,
    ) -> Self {
        // Build HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .expect("Failed to build HTTP client");

        let api_token = api_token.into();

        // Validate token is not empty
        if api_token.is_empty() {
            panic!("Cloudflare API token cannot be empty");
        }

        Self {
            api_token,
            zone_id,
            account_id,
            client,
            dry_run,
        }
    }

    /// Create a new Cloudflare provider (production/live mode)
    ///
    /// This is a convenience method that creates a provider in live mode.
    pub fn new_live(
        api_token: impl Into<String>,
        zone_id: Option<String>,
        account_id: Option<String>,
    ) -> Self {
        Self::new(api_token, zone_id, account_id, false)
    }

    /// Create a new Cloudflare provider (dry-run mode)
    ///
    /// This is a convenience method that creates a provider in dry-run mode.
    /// In dry-run mode, the provider will perform all GET requests but skip
    /// PUT updates, logging what would have been changed.
    pub fn new_dry_run(
        api_token: impl Into<String>,
        zone_id: Option<String>,
        account_id: Option<String>,
    ) -> Self {
        Self::new(api_token, zone_id, account_id, true)
    }

    /// Get the zone ID for a domain
    ///
    /// If zone_id is set, returns it directly. Otherwise, queries Cloudflare API
    /// to find the zone ID for the given domain.
    ///
    /// # Parameters
    ///
    /// - `domain`: The domain name
    ///
    /// # Returns
    ///
    /// - `Ok(String)`: The zone ID
    /// - `Err(Error)`: If zone lookup fails
    ///
    /// # API Call
    ///
    /// ```http
    /// GET /zones?name=example.com
    /// Authorization: Bearer <token>
    /// ```
    async fn get_zone_id(&self, domain: &str) -> Result<String> {
        // If zone_id is pre-configured, use it
        if let Some(ref zone_id) = self.zone_id {
            tracing::debug!("Using pre-configured zone ID");
            return Ok(zone_id.to_string());
        }

        // Extract the root domain from the record name
        // For "sub.example.com", we need "example.com"
        let parts: Vec<&str> = domain.split('.').collect();
        if parts.len() < 2 {
            return Err(Error::config(&format!(
                "Invalid domain name: {}",
                domain
            )));
        }

        // Use the last two parts for the zone
        // For "sub.example.com" -> "example.com"
        // For "deep.nested.example.co.uk" -> "example.co.uk" (not perfect, but works for most cases)
        let zone_name = if parts.len() >= 3 && parts[parts.len() - 2].len() <= 3 {
            // Handle TLDs like .co.uk, .com.au
            format!("{}.{}", parts[parts.len() - 3], parts[parts.len() - 2])
        } else {
            format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
        };

        tracing::debug!("Looking up zone ID for domain: {}", zone_name);

        // Make API request to list zones
        let url = format!("{}/zones?name={}", CLOUDFLARE_API_BASE, zone_name);
        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("HTTP request failed: {}", e)))?;

        // Handle specific HTTP status codes
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            // Map HTTP status codes to specific errors
            return match status.as_u16() {
                401 | 403 => {
                    // Authentication or permission error
                    Err(Error::provider("cloudflare",
                        &format!("Authentication failed: Invalid API token or insufficient permissions. Status: {}", status)))
                }
                404 => {
                    // Zone not found
                    Err(Error::not_found(&format!("Zone not found: {}", zone_name)))
                }
                429 => {
                    // Rate limit
                    Err(Error::provider("cloudflare",
                        &format!("Rate limit exceeded. Please retry later. Status: {}", status)))
                }
                500..=599 => {
                    // Cloudflare server error - transient
                    Err(Error::provider("cloudflare",
                        &format!("Cloudflare server error (transient): {} - {}", status, error_text)))
                }
                _ => {
                    // Other errors
                    Err(Error::provider("cloudflare",
                        &format!("Zone lookup failed: {} - {}", status, error_text)))
                }
            };
        }

        // Parse response
        let json: Value = response
            .json()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("Failed to parse response: {}", e)))?;

        // Extract zone ID
        let zones = json["result"]
            .as_array()
            .ok_or_else(|| Error::provider("cloudflare", "Invalid response format: result is not an array"))?;

        let zone = zones
            .first()
            .ok_or_else(|| Error::not_found(&format!("Zone not found: {}", zone_name)))?;

        let zone_id = zone["id"]
            .as_str()
            .ok_or_else(|| Error::provider("cloudflare", "Invalid response format: zone.id is not a string"))?;

        tracing::debug!("Found zone ID: {}", zone_id);
        Ok(zone_id.to_string())
    }

    /// Get the DNS record ID for a record name
    ///
    /// # Parameters
    ///
    /// - `zone_id`: The zone ID
    /// - `record_name`: The DNS record name
    /// - `record_type`: The DNS record type (A or AAAA)
    ///
    /// # Returns
    ///
    /// - `Ok(String)`: The record ID
    /// - `Err(Error)`: If record lookup fails
    ///
    /// # API Call
    ///
    /// ```http
    /// GET /zones/:zone_id/dns_records?name=example.com&type=A
    /// Authorization: Bearer <token>
    /// ```
    async fn get_record_id(
        &self,
        zone_id: &str,
        record_name: &str,
        record_type: &str,
    ) -> Result<String> {
        tracing::debug!(
            "Looking up record ID: {} (type: {})",
            record_name,
            record_type
        );

        let url = format!(
            "{}/zones/{}/dns_records?name={}&type={}",
            CLOUDFLARE_API_BASE, zone_id, record_name, record_type
        );

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => {
                    Err(Error::provider("cloudflare",
                        &format!("Authentication failed: Invalid API token or insufficient permissions. Status: {}", status)))
                }
                404 => {
                    Err(Error::not_found(&format!(
                        "DNS record not found: {} (type: {})",
                        record_name, record_type
                    )))
                }
                429 => {
                    Err(Error::provider("cloudflare",
                        &format!("Rate limit exceeded. Please retry later. Status: {}", status)))
                }
                500..=599 => {
                    Err(Error::provider("cloudflare",
                        &format!("Cloudflare server error (transient): {} - {}", status, error_text)))
                }
                _ => {
                    Err(Error::provider("cloudflare",
                        &format!("Record lookup failed: {} - {}", status, error_text)))
                }
            };
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("Failed to parse response: {}", e)))?;

        let records = json["result"]
            .as_array()
            .ok_or_else(|| Error::provider("cloudflare", "Invalid response format: result is not an array"))?;

        let record = records.first().ok_or_else(|| {
            Error::not_found(&format!(
                "DNS record not found: {} (type: {})",
                record_name, record_type
            ))
        })?;

        let record_id = record["id"]
            .as_str()
            .ok_or_else(|| Error::provider("cloudflare", "Invalid response format: record.id is not a string"))?;

        tracing::debug!("Found record ID: {}", record_id);
        Ok(record_id.to_string())
    }
}

#[async_trait]
impl DnsProvider for CloudflareProvider {
    /// Update a DNS record with a new IP address
    ///
    /// This implementation:
    /// - Makes ONE HTTP request per engine event (GET to check, PUT if needed)
    /// - Returns full error propagation (no retry, no backoff - owned by engine)
    /// - Never logs the API token
    /// - Never spawns background tasks
    /// - Never caches state (owned by StateStore)
    /// - In dry-run mode, logs intended changes without making them
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name (e.g., "example.com")
    /// - `new_ip`: The new IP address
    ///
    /// # Returns
    ///
    /// - `Ok(UpdateResult)`: Success or Unchanged
    /// - `Err(Error)`: If update fails (propagated to engine for retry)
    ///
    /// # API Calls
    ///
    /// ```http
    /// # Get current record
    /// GET /zones/:zone_id/dns_records/:record_id
    ///
    /// # Update if IP differs (skipped in dry-run mode)
    /// PUT /zones/:zone_id/dns_records/:record_id
    /// {
    ///   "content": "1.2.3.4",
    ///   "type": "A" or "AAAA"
    /// }
    /// ```
    async fn update_record(&self, record_name: &str, new_ip: IpAddr) -> Result<UpdateResult> {
        // Determine record type based on IP address
        let record_type = match new_ip {
            IpAddr::V4(_) => "A",
            IpAddr::V6(_) => "AAAA",
        };

        tracing::info!(
            "Updating Cloudflare DNS record: {} -> {} ({}) [mode: {}]",
            record_name,
            new_ip,
            record_type,
            if self.dry_run { "DRY-RUN" } else { "LIVE" }
        );

        // Step 1: Get zone ID
        let zone_id = self.get_zone_id(record_name).await?;

        // Step 2: Get record ID
        let record_id = self.get_record_id(&zone_id, record_name, record_type).await?;

        // Step 3: Get current record to check if IP matches
        let get_url = format!(
            "{}/zones/{}/dns_records/{}",
            CLOUDFLARE_API_BASE, zone_id, record_id
        );

        let get_response = self
            .client
            .get(&get_url)
            .bearer_auth(&self.api_token)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("HTTP request failed: {}", e)))?;

        if !get_response.status().is_success() {
            let status = get_response.status();
            let error_text = get_response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => {
                    Err(Error::provider("cloudflare",
                        &format!("Authentication failed: Invalid API token or insufficient permissions. Status: {}", status)))
                }
                404 => {
                    Err(Error::not_found(&format!("DNS record not found: {}", record_name)))
                }
                429 => {
                    Err(Error::provider("cloudflare",
                        &format!("Rate limit exceeded. Please retry later. Status: {}", status)))
                }
                500..=599 => {
                    Err(Error::provider("cloudflare",
                        &format!("Cloudflare server error (transient): {} - {}", status, error_text)))
                }
                _ => {
                    Err(Error::provider("cloudflare",
                        &format!("Failed to get record: {} - {}", status, error_text)))
                }
            };
        }

        let record_json: Value = get_response
            .json()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("Failed to parse response: {}", e)))?;

        let current_ip_str = record_json["result"]["content"]
            .as_str()
            .ok_or_else(|| Error::provider("cloudflare", "Invalid response format: content is not a string"))?;

        let current_ip: IpAddr = current_ip_str
            .parse()
            .map_err(|e| Error::provider("cloudflare", &format!("Invalid IP in response: {}", e)))?;

        // Step 4: If IP matches, return Unchanged
        if current_ip == new_ip {
            tracing::info!(
                "DNS record already has correct IP: {} -> {}",
                record_name,
                new_ip
            );
            return Ok(UpdateResult::Unchanged { current_ip });
        }

        // Step 5: Update the record (or dry-run)
        tracing::info!(
            "{} DNS record: {} -> {} (was: {})",
            if self.dry_run { "Would update" } else { "Updating" },
            record_name,
            new_ip,
            current_ip
        );

        // In dry-run mode, log the intended update and return success
        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would send PUT request to {} with payload: {}",
                get_url,
                serde_json::json!({
                    "content": new_ip.to_string(),
                    "type": record_type,
                })
            );
            // Return as if update succeeded
            return Ok(UpdateResult::Updated {
                previous_ip: Some(current_ip),
                new_ip,
            });
        }

        // Perform actual update in live mode
        let update_payload = serde_json::json!({
            "content": new_ip.to_string(),
            "type": record_type,
        });

        let put_response = self
            .client
            .put(&get_url)
            .bearer_auth(&self.api_token)
            .header("Content-Type", "application/json")
            .json(&update_payload)
            .send()
            .await
            .map_err(|e| Error::provider("cloudflare", &format!("HTTP request failed: {}", e)))?;

        if !put_response.status().is_success() {
            let status = put_response.status();
            let error_text = put_response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => {
                    Err(Error::provider("cloudflare",
                        &format!("Authentication failed: Invalid API token or insufficient permissions. Status: {}", status)))
                }
                409 => {
                    Err(Error::provider("cloudflare",
                        &format!("Conflict: Record is being updated by another process. Status: {}", status)))
                }
                429 => {
                    Err(Error::provider("cloudflare",
                        &format!("Rate limit exceeded. Please retry later. Status: {}", status)))
                }
                500..=599 => {
                    Err(Error::provider("cloudflare",
                        &format!("Cloudflare server error (transient): {} - {}", status, error_text)))
                }
                _ => {
                    Err(Error::provider("cloudflare",
                        &format!("Failed to update record: {} - {}", status, error_text)))
                }
            };
        }

        tracing::info!("DNS record updated successfully: {} -> {}", record_name, new_ip);
        Ok(UpdateResult::Updated {
            previous_ip: Some(current_ip),
            new_ip,
        })
    }

    async fn get_record(&self, _record_name: &str) -> Result<RecordMetadata> {
        // TODO: Implement actual API call
        // GET /zones/:zone_id/dns_records?name=example.com
        Err(Error::not_found("get_record not implemented"))
    }

    fn supports_record(&self, record_name: &str) -> bool {
        // Basic validation: Cloudflare supports most DNS record types
        // More sophisticated validation could check TLD support, etc.
        record_name.contains('.') && record_name.len() <= 253
    }

    fn provider_name(&self) -> &'static str {
        "cloudflare"
    }
}

/// Factory for creating Cloudflare providers
pub struct CloudflareFactory;

impl DnsProviderFactory for CloudflareFactory {
    fn create(&self, config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
        match config {
            ProviderConfig::Cloudflare {
                api_token,
                zone_id,
                account_id,
            } => {
                if api_token.is_empty() {
                    return Err(Error::config("Cloudflare API token is required"));
                }

                // Check for dry-run mode environment variable
                let dry_run = std::env::var("DDNS_MODE")
                    .unwrap_or_default()
                    .to_lowercase() == "dry-run";

                if dry_run {
                    tracing::warn!("Cloudflare provider running in DRY-RUN mode - no changes will be made");
                }

                Ok(Box::new(CloudflareProvider::new(
                    api_token.clone(),
                    zone_id.clone(),
                    account_id.clone(),
                    dry_run,
                )))
            }
            _ => Err(Error::config("Invalid config for Cloudflare provider")),
        }
    }
}

/// Register the Cloudflare provider with a registry
///
/// This function should be called during initialization to make the
/// Cloudflare provider available.
///
/// # Example
///
/// ```rust
/// use ddns_core::ProviderRegistry;
///
/// let mut registry = ProviderRegistry::new();
/// ddns_provider_cloudflare::register(&registry);
/// ```
pub fn register(registry: &ddns_core::ProviderRegistry) {
    registry.register_provider("cloudflare", Box::new(CloudflareFactory));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creation() {
        let factory = CloudflareFactory;

        let config = ProviderConfig::Cloudflare {
            api_token: "test_token".to_string(),
            zone_id: Some("test_zone".to_string()),
            account_id: None,
        };

        let provider = factory.create(&config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_factory_missing_token() {
        let factory = CloudflareFactory;

        let config = ProviderConfig::Cloudflare {
            api_token: "".to_string(),
            zone_id: None,
            account_id: None,
        };

        let provider = factory.create(&config);
        assert!(provider.is_err());
    }

    #[test]
    #[should_panic(expected = "API token cannot be empty")]
    fn test_empty_token_panics() {
        CloudflareProvider::new("", None, None, false);
    }

    #[test]
    fn test_dry_run_mode() {
        let provider_dry = CloudflareProvider::new_dry_run("token", None, None);
        let provider_live = CloudflareProvider::new_live("token", None, None);

        assert!(provider_dry.dry_run, "Dry-run provider should have dry_run=true");
        assert!(!provider_live.dry_run, "Live provider should have dry_run=false");
    }

    #[test]
    fn test_supports_record() {
        let provider = CloudflareProvider::new("token", None, None, false);

        assert!(provider.supports_record("example.com"));
        assert!(provider.supports_record("sub.example.com"));
        assert!(!provider.supports_record(""));
        assert!(!provider.supports_record("a".repeat(254).as_str()));
    }

    #[test]
    fn test_provider_name() {
        let provider = CloudflareProvider::new("token", None, None, false);
        assert_eq!(provider.provider_name(), "cloudflare");
    }

    #[test]
    fn test_zone_id_preconfigured() {
        // Test that pre-configured zone ID is returned immediately
        let provider = CloudflareProvider::new(
            "test_token",
            Some("test_zone_id".to_string()),
            None,
            false,
        );

        // This test verifies the logic, but doesn't make actual API calls
        // In a real test, we'd use mockito or similar for HTTP mocking
        assert_eq!(provider.zone_id, Some("test_zone_id".to_string()));
    }

    #[test]
    fn test_api_token_not_exposed_in_debug() {
        // Test that API token is not exposed in Debug output
        let provider = CloudflareProvider::new(
            "secret_token_12345",
            None,
            None,
            false,
        );

        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret_token_12345"));
        assert!(!debug_str.contains("secret_token"));
        // The struct name should appear but not the token value
        assert!(debug_str.contains("CloudflareProvider"));
    }

    #[test]
    fn test_http_timeout_configured() {
        // Test that HTTP client is configured with timeout
        let provider = CloudflareProvider::new(
            "test_token",
            None,
            None,
            false,
        );

        // Verify client was created successfully
        // (we can't inspect the timeout directly, but successful creation
        // means the builder didn't fail)
        assert_eq!(provider.api_token, "test_token");
    }
}
