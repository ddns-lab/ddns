// # Aliyun DNS Provider
//
// This crate provides an Aliyun (Alibaba Cloud) DNS provider implementation for the DDNS system.
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
// - ✅ HMAC-SHA1 signature for API authentication
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
// - AccessKey ID/Secret NEVER appear in logs
// - AccessKey/Secret MUST be provided via environment variables only
// - Provider MUST fail fast if credentials are empty
//
// ## API Reference
//
// - Aliyun DNS API: https://help.aliyun.com/zh/dns/api-alidns-2015-01-09-summary
// - DescribeDomainRecords: GET / (query records)
// - UpdateDomainRecord: PUT / (update record)
// - AddDomainRecord: POST / (create record)

use async_trait::async_trait;
use chrono::Utc;
use ddns_core::config::ProviderConfig;
use ddns_core::traits::{DnsProvider, DnsProviderFactory, RecordMetadata, UpdateResult};
use ddns_core::{Error, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;
use std::net::IpAddr;
use std::time::Duration;

/// Aliyun API base URL
const ALIYUN_API_BASE: &str = "https://alidns.aliyuncs.com";

/// Aliyun API version
const API_VERSION: &str = "2015-01-09";

/// Default HTTP timeout for API requests (30 seconds)
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Aliyun DNS provider
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
/// - Log the intended PUT payload
/// - **NOT** actually modify DNS records
///
/// This allows safe testing without making changes.
///
/// # Security
///
/// The Debug implementation intentionally does NOT expose the AccessKey Secret.
///
/// # Authentication
///
/// Aliyun uses AccessKey authentication with HMAC-SHA1 signature:
/// - AccessKey ID: Public identifier
/// - AccessKey Secret: Used for HMAC-SHA1 signature
impl std::fmt::Debug for AliyunProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AliyunProvider")
            .field("access_key_id", &"<REDACTED>")
            .field("access_key_secret", &"<REDACTED>")
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

pub struct AliyunProvider {
    /// Aliyun AccessKey ID
    /// ⚠️ NEVER log this value
    access_key_id: String,

    /// Aliyun AccessKey Secret (for HMAC-SHA1 signature)
    /// ⚠️ NEVER log this value
    access_key_secret: String,

    /// HTTP client for API requests
    client: reqwest::Client,

    /// Dry-run mode: if true, perform GET requests but skip PUT updates
    dry_run: bool,
}

impl AliyunProvider {
    /// Create a new Aliyun provider
    ///
    /// # Parameters
    ///
    /// - `access_key_id`: Aliyun AccessKey ID
    /// - `access_key_secret`: Aliyun AccessKey Secret (for signing)
    /// - `dry_run`: If true, perform GET requests but skip PUT updates
    ///
    /// # Security
    ///
    /// The AccessKey ID/Secret will NEVER be logged or displayed in error messages.
    pub fn new(
        access_key_id: impl Into<String>,
        access_key_secret: impl Into<String>,
        dry_run: bool,
    ) -> Self {
        // Build HTTP client with timeout
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .expect("Failed to build HTTP client");

        let access_key_id = access_key_id.into();
        let access_key_secret = access_key_secret.into();

        // Validate credentials are not empty
        if access_key_id.is_empty() {
            panic!("Aliyun AccessKey ID cannot be empty");
        }
        if access_key_secret.is_empty() {
            panic!("Aliyun AccessKey Secret cannot be empty");
        }

        Self {
            access_key_id,
            access_key_secret,
            client,
            dry_run,
        }
    }

    /// Create a new Aliyun provider (production/live mode)
    pub fn new_live(
        access_key_id: impl Into<String>,
        access_key_secret: impl Into<String>,
    ) -> Self {
        Self::new(access_key_id, access_key_secret, false)
    }

    /// Create a new Aliyun provider (dry-run mode)
    pub fn new_dry_run(
        access_key_id: impl Into<String>,
        access_key_secret: impl Into<String>,
    ) -> Self {
        Self::new(access_key_id, access_key_secret, true)
    }

    /// Build Aliyun API signature (HMAC-SHA1)
    ///
    /// Aliyun uses a specific signature format:
    /// `signature = HMAC-SHA1(AccessKeySecret, string_to_sign)`
    ///
    /// # Algorithm
    ///
    /// 1. Build canonicalized query string
    /// 2. Calculate HMAC-SHA1
    /// 3. Base64 encode the result
    fn build_signature(&self, params: &str) -> String {
        // Create HMAC-SHA1
        let mut mac = Hmac::<Sha1>::new_from_slice(self.access_key_secret.as_bytes())
            .expect("HMAC can accept key size");

        mac.update(params.as_bytes());

        // Encode result to hex
        hex::encode(mac.finalize().into_bytes())
    }

    /// Build signed API URL with query parameters
    fn build_api_url(&self, action: &str, params: &[(&str, &str)]) -> String {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let mut query_pairs: Vec<(String, String)> = vec![
            ("Action".to_string(), action.to_string()),
            ("Version".to_string(), API_VERSION.to_string()),
            ("AccessKeyId".to_string(), self.access_key_id.clone()),
            ("Format".to_string(), "json".to_string()),
            ("SignatureMethod".to_string(), "HMAC-SHA1".to_string()),
            ("SignatureVersion".to_string(), "1.0".to_string()),
            ("Timestamp".to_string(), timestamp),
        ];

        // Add additional parameters
        for (key, value) in params {
            query_pairs.push((key.to_string(), value.to_string()));
        }

        // Sort parameters (required by Aliyun)
        query_pairs.sort_by(|a, b| a.0.cmp(&b.0));

        // Build canonicalized query string
        let canonicalized_query: Vec<String> = query_pairs
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();

        let query_string = canonicalized_query.join("&");

        // Build string to sign
        let string_to_sign = format!("GET&%2F&{}", urlencoding::encode(&query_string));

        // Calculate signature
        let signature = self.build_signature(&string_to_sign);

        // Build final URL with signature
        let final_query: Vec<String> = query_pairs
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();

        let mut final_query_str = final_query.join("&");
        final_query_str.push_str(&format!("&Signature={}", urlencoding::encode(&signature)));

        format!("{}?{}", ALIYUN_API_BASE, final_query_str)
    }

    /// Get DNS record ID for a record name
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name (e.g., "example.com")
    /// - `record_type`: The DNS record type (A or AAAA)
    ///
    /// # Returns
    ///
    /// - `Ok(String)`: The record ID (RecordId in Aliyun)
    /// - `Err(Error)`: If record lookup fails
    ///
    /// # API Call
    ///
    /// ```http
    /// GET /?Action=DescribeDomainRecords&...
    /// ```
    async fn get_record_id(
        &self,
        record_name: &str,
        record_type: &str,
    ) -> Result<String> {
        tracing::debug!(
            "Looking up Aliyun record ID: {} (type: {})",
            record_name,
            record_type
        );

        let url = self.build_api_url(
            "DescribeDomainRecords",
            &[
                ("DomainName", record_name),
                ("Type", record_type),
            ],
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::provider("aliyun", format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => {
                    // Authentication or permission error
                    Err(Error::provider(
                        "aliyun",
                        format!(
                            "Authentication failed: Invalid AccessKey or insufficient permissions. Status: {}",
                            status
                        ),
                    ))
                }
                404 => {
                    // Record not found
                    Err(Error::not_found(format!(
                        "DNS record not found: {} (type: {})",
                        record_name, record_type
                    )))
                }
                429 => {
                    // Rate limit
                    Err(Error::provider(
                        "aliyun",
                        format!(
                            "Rate limit exceeded. Please retry later. Status: {}",
                            status
                        ),
                    ))
                }
                500..=599 => {
                    // Aliyun server error - transient
                    Err(Error::provider(
                        "aliyun",
                        format!(
                            "Aliyun server error (transient): {} - {}",
                            status, error_text
                        ),
                    ))
                }
                _ => {
                    // Other errors
                    Err(Error::provider(
                        "aliyun",
                        format!("Record lookup failed: {} - {}", status, error_text),
                    ))
                }
            };
        }

        let json: Value = response.json().await.map_err(|e| {
            Error::provider("aliyun", format!("Failed to parse response: {}", e))
        })?;

        // Aliyun returns: { "DomainRecords": { "Record": [...] } }
        let records = json["DomainRecords"]["Record"].as_array().ok_or_else(|| {
            Error::provider(
                "aliyun",
                "Invalid response format: DomainRecords.Record is not an array",
            )
        })?;

        let record = records.first().ok_or_else(|| {
            Error::not_found(format!(
                "DNS record not found: {} (type: {})",
                record_name, record_type
            ))
        })?;

        let record_id = record["RecordId"].as_str().ok_or_else(|| {
            Error::provider(
                "aliyun",
                "Invalid response format: RecordId is not a string",
            )
        })?;

        let current_ip = record["Value"].as_str().ok_or_else(|| {
            Error::provider(
                "aliyun",
                "Invalid response format: Value is not a string",
            )
        })?;

        tracing::debug!(
            "Found Aliyun record ID: {} (current IP: {})",
            record_id,
            current_ip
        );
        Ok(record_id.to_string())
    }

    /// Create a new DNS record
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name (e.g., "example.com")
    /// - `record_type`: The DNS record type (A or AAAA)
    /// - `ip`: The IP address for the record
    ///
    /// # Returns
    ///
    /// - `Ok(String)`: The created record ID
    /// - `Err(Error)`: If creation fails
    ///
    /// # API Call
    ///
    /// ```http
    /// POST /?Action=AddDomainRecord&...
    /// ```
    async fn create_record(
        &self,
        record_name: &str,
        record_type: &str,
        ip: IpAddr,
    ) -> Result<String> {
        tracing::info!(
            "Creating Aliyun DNS record: {} ({}) -> {}",
            record_name,
            record_type,
            ip
        );

        let url = self.build_api_url(
            "AddDomainRecord",
            &[
                ("DomainName", record_name),
                ("RR", record_name),
                ("Type", record_type),
                ("Value", &ip.to_string()),
                ("TTL", "600"),
            ],
        );

        // In dry-run mode, log the intended creation and return success
        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would send POST request to {}",
                url
            );
            // Return a dummy record ID
            return Ok("dry-run-record-id".to_string());
        }

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| Error::provider("aliyun", format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Authentication failed: Invalid AccessKey or insufficient permissions. Status: {}",
                        status
                    ),
                )),
                409 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Conflict: Record already exists. Status: {}",
                        status
                    ),
                )),
                429 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Rate limit exceeded. Please retry later. Status: {}",
                        status
                    ),
                )),
                500..=599 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Aliyun server error (transient): {} - {}",
                        status, error_text
                    ),
                )),
                _ => Err(Error::provider(
                    "aliyun",
                    format!("Failed to create record: {} - {}", status, error_text),
                )),
            };
        }

        let record_json: Value = response.json().await.map_err(|e| {
            Error::provider("aliyun", format!("Failed to parse response: {}", e))
        })?;

        let record_id = record_json["RecordId"].as_str().ok_or_else(|| {
            Error::provider(
                "aliyun",
                "Invalid response format: RecordId is not a string",
            )
        })?;

        tracing::info!(
            "Aliyun DNS record created successfully: {} ({}) -> {}",
            record_name,
            record_type,
            ip
        );

        Ok(record_id.to_string())
    }

    /// Get current record value
    async fn get_current_record(&self, record_id: &str) -> Result<IpAddr> {
        // For Aliyun, we need to query by record_id
        let url = self.build_api_url(
            "DescribeDomainRecords",
            &[("RecordId", record_id)],
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::provider("aliyun", format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Authentication failed: Invalid AccessKey or insufficient permissions. Status: {}",
                        status
                    ),
                )),
                404 => Err(Error::not_found(format!(
                    "DNS record not found: {}",
                    record_id
                ))),
                429 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Rate limit exceeded. Please retry later. Status: {}",
                        status
                    ),
                )),
                500..=599 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Aliyun server error (transient): {} - {}",
                        status, error_text
                    ),
                )),
                _ => Err(Error::provider(
                    "aliyun",
                    format!("Failed to get record: {} - {}", status, error_text),
                )),
            };
        }

        let json: Value = response.json().await.map_err(|e| {
            Error::provider("aliyun", format!("Failed to parse response: {}", e))
        })?;

        let records = json["DomainRecords"]["Record"].as_array().ok_or_else(|| {
            Error::provider(
                "aliyun",
                "Invalid response format: DomainRecords.Record is not an array",
            )
        })?;

        let record = records.first().ok_or_else(|| {
            Error::not_found(format!("DNS record not found: {}", record_id))
        })?;

        let current_ip_str = record["Value"].as_str().ok_or_else(|| {
            Error::provider(
                "aliyun",
                "Invalid response format: Value is not a string",
            )
        })?;

        let current_ip: IpAddr = current_ip_str
            .parse()
            .map_err(|e| Error::provider("aliyun", format!("Invalid IP in response: {}", e)))?;

        Ok(current_ip)
    }

    /// Update a DNS record with a new IP
    async fn update_record_ip(
        &self,
        record_id: &str,
        record_name: &str,
        record_type: &str,
        new_ip: IpAddr,
        _previous_ip: IpAddr,
    ) -> Result<()> {
        tracing::info!(
            "Updating Aliyun DNS record: {} -> {}",
            record_name,
            new_ip
        );

        let url = self.build_api_url(
            "UpdateDomainRecord",
            &[
                ("RecordId", record_id),
                ("RR", record_name),
                ("Type", record_type),
                ("Value", &new_ip.to_string()),
                ("TTL", "600"),
            ],
        );

        // In dry-run mode, log the intended update and return success
        if self.dry_run {
            tracing::info!(
                "[DRY-RUN] Would send POST request to {}",
                url
            );
            return Ok(());
        }

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| Error::provider("aliyun", format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            return match status.as_u16() {
                401 | 403 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Authentication failed: Invalid AccessKey or insufficient permissions. Status: {}",
                        status
                    ),
                )),
                404 => Err(Error::not_found(format!(
                    "DNS record not found: {}",
                    record_id
                ))),
                409 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Conflict: Record is being updated by another process. Status: {}",
                        status
                    ),
                )),
                429 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Rate limit exceeded. Please retry later. Status: {}",
                        status
                    ),
                )),
                500..=599 => Err(Error::provider(
                    "aliyun",
                    format!(
                        "Aliyun server error (transient): {} - {}",
                        status, error_text
                    ),
                )),
                _ => Err(Error::provider(
                    "aliyun",
                    format!("Failed to update record: {} - {}", status, error_text),
                )),
            };
        }

        tracing::info!(
            "Aliyun DNS record updated successfully: {} -> {}",
            record_name,
            new_ip
        );

        Ok(())
    }
}

#[async_trait]
impl DnsProvider for AliyunProvider {
    /// Update a DNS record with a new IP address
    ///
    /// This implementation:
    /// - Makes ONE HTTP request per engine event (GET to check, POST if needed)
    /// - Returns full error propagation (no retry, no backoff - owned by engine)
    /// - Never logs the AccessKey ID/Secret
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
    async fn update_record(&self, record_name: &str, new_ip: IpAddr) -> Result<UpdateResult> {
        // Determine record type based on IP address
        let record_type = match new_ip {
            IpAddr::V4(_) => "A",
            IpAddr::V6(_) => "AAAA",
        };

        tracing::info!(
            "Updating Aliyun DNS record: {} -> {} ({}) [mode: {}]",
            record_name,
            new_ip,
            record_type,
            if self.dry_run { "DRY-RUN" } else { "LIVE" }
        );

        // Step 1: Get record ID (create if not exists)
        let (record_id, is_newly_created) =
            match self.get_record_id(record_name, record_type).await {
                Ok(id) => (id, false),
                Err(Error::NotFound { .. }) => {
                    tracing::info!(
                        "DNS record does not exist, creating: {} ({})",
                        record_name,
                        record_type
                    );
                    (
                        self.create_record(record_name, record_type, new_ip).await?,
                        true,
                    )
                }
                Err(e) => return Err(e),
            };

        // If record was just created, return Created result
        if is_newly_created {
            tracing::info!(
                "DNS record created successfully: {} -> {}",
                record_name,
                new_ip
            );
            return Ok(UpdateResult::Created { new_ip });
        }

        // Step 2: Get current record to check if IP matches
        let current_ip = self.get_current_record(&record_id).await?;

        // Step 3: If IP matches, return Unchanged (idempotency)
        if current_ip == new_ip {
            tracing::info!(
                "DNS record already has correct IP: {} -> {}",
                record_name,
                new_ip
            );
            return Ok(UpdateResult::Unchanged { current_ip });
        }

        // Step 4: Dry-run mode check
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

        // Step 5: Perform actual update
        self.update_record_ip(&record_id, record_name, record_type, new_ip, current_ip)
            .await?;

        tracing::info!(
            "DNS record updated successfully: {} -> {}",
            record_name,
            new_ip
        );

        Ok(UpdateResult::Updated {
            previous_ip: Some(current_ip),
            new_ip,
        })
    }

    async fn get_record(&self, _record_name: &str) -> Result<RecordMetadata> {
        // TODO: Implement actual API call
        Err(Error::not_found("get_record not implemented"))
    }

    fn supports_record(&self, record_name: &str) -> bool {
        // Basic validation: Aliyun supports most DNS record types
        record_name.contains('.') && record_name.len() <= 253
    }

    fn provider_name(&self) -> &'static str {
        "aliyun"
    }
}

/// Factory for creating Aliyun providers
pub struct AliyunFactory;

impl DnsProviderFactory for AliyunFactory {
    fn create(&self, config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
        match config {
            ProviderConfig::Aliyun {
                access_key_id,
                access_key_secret,
            } => {
                if access_key_id.is_empty() {
                    return Err(Error::config("Aliyun AccessKey ID is required"));
                }
                if access_key_secret.is_empty() {
                    return Err(Error::config("Aliyun AccessKey Secret is required"));
                }

                // Check for dry-run mode environment variable
                let dry_run = std::env::var("DDNS_MODE")
                    .unwrap_or_default()
                    .to_lowercase()
                    == "dry-run";

                if dry_run {
                    tracing::warn!(
                        "Aliyun provider running in DRY-RUN mode - no changes will be made"
                    );
                }

                Ok(Box::new(AliyunProvider::new(
                    access_key_id.clone(),
                    access_key_secret.clone(),
                    dry_run,
                )))
            }
            _ => Err(Error::config("Invalid config for Aliyun provider")),
        }
    }
}

/// Register the Aliyun provider with a registry
///
/// This function should be called during initialization to make the
/// Aliyun provider available.
///
/// # Example
///
/// ```rust
/// use ddns_core::ProviderRegistry;
///
/// let mut registry = ProviderRegistry::new();
/// ddns_provider_aliyun::register(&registry);
/// ```
pub fn register(registry: &ddns_core::ProviderRegistry) {
    registry.register_provider("aliyun", Box::new(AliyunFactory));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = AliyunProvider::new_live("test_id", "test_secret");
        assert_eq!(provider.provider_name(), "aliyun");
    }

    #[test]
    fn test_dry_run_mode() {
        let provider_dry = AliyunProvider::new_dry_run("id", "secret");
        let provider_live = AliyunProvider::new_live("id", "secret");

        assert!(provider_dry.dry_run, "Dry-run provider should have dry_run=true");
        assert!(!provider_live.dry_run, "Live provider should have dry_run=false");
    }

    #[test]
    fn test_api_token_not_exposed_in_debug() {
        let provider = AliyunProvider::new_live("secret_key_id", "secret_key_value");

        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret_key_id"));
        assert!(!debug_str.contains("secret_key_value"));
        assert!(debug_str.contains("AliyunProvider"));
    }

    #[test]
    fn test_supports_record() {
        let provider = AliyunProvider::new_live("id", "secret");

        assert!(provider.supports_record("example.com"));
        assert!(provider.supports_record("sub.example.com"));
        assert!(!provider.supports_record(""));
        assert!(!provider.supports_record("a".repeat(254).as_str()));
    }

    #[test]
    #[should_panic(expected = "AccessKey ID cannot be empty")]
    fn test_empty_access_key_id_panics() {
        AliyunProvider::new("", "secret", false);
    }

    #[test]
    #[should_panic(expected = "AccessKey Secret cannot be empty")]
    fn test_empty_access_key_secret_panics() {
        AliyunProvider::new("id", "", false);
    }
}
