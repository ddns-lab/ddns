// # DNS Provider Trait
//
// Defines the interface for updating DNS records via provider APIs.
//
// ## Implementations
//
// - Cloudflare: `ddns-provider-cloudflare` crate
// - Future: Route53, DigitalOcean, GoDaddy, etc.
//
// ## Usage
//
// ```rust,ignore
// use ddns_core::DnsProvider;
//
// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let provider = /* DnsProvider implementation */;
//
//     // Update a DNS record
//     provider.update_record(
//         "example.com",
//         &std::net::IpAddr::from([192, 168, 1, 1]),
//     ).await?;
//
//     Ok(())
// }
// ```

use async_trait::async_trait;
use std::net::IpAddr;

/// Result of a DNS update operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateResult {
    /// Record was successfully updated
    Updated {
        /// The previous IP address
        previous_ip: Option<IpAddr>,
        /// The new IP address
        new_ip: IpAddr,
    },
    /// Record already had the correct IP (no-op)
    Unchanged {
        /// The current IP address
        current_ip: IpAddr,
    },
    /// Record was created (didn't exist before)
    Created {
        /// The created IP address
        new_ip: IpAddr,
    },
}

/// Metadata about a DNS record
#[derive(Debug, Clone)]
pub struct RecordMetadata {
    /// The record ID (provider-specific)
    pub id: String,
    /// The record name
    pub name: String,
    /// The current IP address
    pub ip: IpAddr,
    /// Time-to-live for the record
    pub ttl: Option<u32>,
    /// Any additional provider-specific metadata
    pub extra: serde_json::Value,
}

/// Trait for DNS provider implementations
///
/// This trait defines the interface for updating DNS records.
/// Implementations must handle the specifics of each provider's API.
///
/// # Thread Safety
///
/// Implementations must be thread-safe and usable across async tasks.
///
/// # Trust Level: Untrusted
///
/// DNS providers are **untrusted** components with strict limitations:
///
/// ## Allowed Capabilities
/// - ✅ Perform HTTP/HTTPS API calls to their endpoints only
/// - ⚠️ Allocate minimal memory (prefer streaming for responses)
/// - ✅ Parse provider-specific responses
/// - ✅ Return success or failure (engine handles retry)
///
/// ## Forbidden Capabilities
/// - ❌ Spawn tasks or threads (violates shutdown determinism)
/// - ❌ Implement retry logic or backoff (owned by `DdnsEngine`)
/// - ❌ Access state store (owned by `DdnsEngine`)
/// - ❌ Access other providers (must be isolated)
/// - ❌ Make scheduling decisions (owned by `DdnsEngine`)
/// - ❌ Cache state beyond single request (owned by `StateStore`)
/// - ❌ Decide whether an update is needed (owned by `DdnsEngine`)
/// - ❌ Perform any I/O other than API calls to their endpoints
///
/// ## Rationale
///
/// Providers are external integrations that should be:
/// - **Isolated**: No knowledge of other providers or system state
/// - **Stateless**: No persistent state between requests
/// - **Single-shot**: Execute one API call per invocation
/// - **Deterministic**: Same input → same output, no hidden behavior
///
/// ## Examples
///
/// ✅ **CORRECT**: Stateless single-shot API call
/// ```rust,ignore
/// async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
///     let response = self.http_client
///         .put(format!("/zones/{}/dns_records/{}", self.zone_id, record))
///         .json(&serde_json::json!({ "content": ip.to_string() }))
///         .send()
///         .await?; // Single API call
///
///     if response.status().is_success() {
///         Ok(UpdateResult::Updated { ... })
///     } else {
///         Err(Error::provider_error("API call failed")) // Engine will retry
///     }
/// }
/// ```
///
/// ❌ **WRONG**: Provider with retry logic
/// ```rust,ignore
/// async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
///     let mut attempts = 0;
///     loop {
///         match self.do_update(record, ip).await {
///             Ok(result) => return Ok(result),
///             Err(e) if attempts < 3 => {
///                 attempts += 1;
///                 tokio::time::sleep(Duration::from_secs(1)).await; // WRONG!
///             }
///             Err(e) => return Err(e),
///         }
///     }
/// }
/// ```
///
/// ## Why No Retry Logic?
///
/// If providers implement their own retry logic:
/// - Engine cannot control retry rate (can cause API storms)
/// - Inconsistent retry behavior across providers
/// - Engine cannot implement intelligent backoff policies
/// - Shutdown determinism is violated (sleeping tasks)
///
/// **Correct approach**: Return an error. The `DdnsEngine` will retry according to its configured policy.
///
/// See `docs/architecture/TRUST_LEVELS.md` for complete trust level definitions.
#[async_trait]
pub trait DnsProvider: Send + Sync {
    /// Update a DNS record with a new IP address
    ///
    /// This method should handle the following cases:
    /// - Record exists with a different IP → Update it
    /// - Record exists with the same IP → Return `UpdateResult::Unchanged`
    /// - Record doesn't exist → Create it (if supported)
    ///
    /// # Idempotency
    ///
    /// This method must be idempotent: calling it multiple times
    /// with the same IP should be safe and result in no additional
    /// changes after the first successful update.
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name (e.g., "example.com" or "sub.example.com")
    /// - `new_ip`: The new IP address to set
    ///
    /// # Returns
    ///
    /// - `Ok(UpdateResult)`: The result of the update operation
    /// - `Err(Error)`: If the update failed
    async fn update_record(
        &self,
        record_name: &str,
        new_ip: IpAddr,
    ) -> Result<UpdateResult, crate::Error>;

    /// Get current metadata for a DNS record
    ///
    /// This method retrieves the current state of a DNS record
    /// from the provider, including its current IP and metadata.
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    ///
    /// # Returns
    ///
    /// - `Ok(RecordMetadata)`: The record's current metadata
    /// - `Err(Error)`: If the record doesn't exist or the request failed
    async fn get_record(
        &self,
        record_name: &str,
    ) -> Result<RecordMetadata, crate::Error>;

    /// Check if this provider supports the given record type
    ///
    /// Some providers may have limitations on record types or names.
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name to check
    ///
    /// # Returns
    ///
    /// `true` if this provider can handle the record, `false` otherwise
    fn supports_record(&self, record_name: &str) -> bool;

    /// Get the provider name (for logging/debugging)
    ///
    /// # Returns
    ///
    /// A static string identifying the provider (e.g., "cloudflare", "route53")
    fn provider_name(&self) -> &'static str;
}

/// Helper trait for constructing DNS providers from configuration
pub trait DnsProviderFactory: Send + Sync {
    /// Create a DnsProvider instance from configuration
    ///
    /// # Parameters
    ///
    /// - `config`: Configuration specific to this provider
    ///
    /// # Returns
    ///
    /// A boxed DnsProvider trait object
    fn create(
        &self,
        config: &crate::config::ProviderConfig,
    ) -> Result<Box<dyn DnsProvider>, crate::Error>;
}
