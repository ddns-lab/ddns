// # HTTP IP Source
//
// This crate provides an HTTP-based IP source for the DDNS system.
//
// ## Purpose
//
// This is a **fallback IP source** for:
// - Non-Linux platforms (macOS, Windows, BSD)
// - CI/CD testing
// - Debugging and validation
//
// ## IMPORTANT: Not Primary on Linux
//
// On Linux, **always prefer Netlink** (ddns-ip-netlink) over HTTP.
// This source is documented as non-primary and should only be used
// when Netlink is unavailable.
//
// ## Architecture
//
// Fetches current IP from external services (e.g., ifconfig.me, icanhazip.com)
// and polls at a configurable interval for changes.

use ddns_core::ProviderRegistry;
use ddns_core::config::IpSourceConfig;
use ddns_core::config::IpVersion as ConfigIpVersion;
use ddns_core::traits::{IpChangeEvent, IpSource, IpSourceFactory, IpVersion as TraitsIpVersion};
use ddns_core::{Error, Result};

use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_stream::Stream;
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Default polling interval for HTTP IP source
const DEFAULT_POLL_INTERVAL_SECS: u64 = 60;

/// Default IP check services (for future failover support)
#[allow(dead_code)]
const DEFAULT_IP_SERVICES: &[&str] = &[
    "https://api.ipify.org",  // 43KB/day free, returns plain text IP
    "https://ifconfig.me/ip", // No rate limit documented
    "https://icanhazip.com",  // No rate limit documented
];

/// HTTP-based IP source (fallback for non-Linux or CI)
pub struct HttpIpSource {
    /// URL to fetch IP from
    url: String,

    /// IP version to monitor
    version: Option<ConfigIpVersion>,

    /// Polling interval
    poll_interval: Duration,

    /// Current IP address (cached)
    current_ip: Arc<Mutex<Option<IpAddr>>>,

    /// HTTP client
    client: reqwest::Client,
}

impl HttpIpSource {
    /// Create a new HTTP IP source
    ///
    /// # Parameters
    ///
    /// - `url`: URL to fetch IP from (e.g., "https://api.ipify.org")
    /// - `version`: IP version to monitor (None = both)
    pub fn new(url: String, version: Option<ConfigIpVersion>) -> Self {
        Self {
            url,
            version,
            poll_interval: Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
            current_ip: Arc::new(Mutex::new(None)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create with custom polling interval
    pub fn with_interval(
        url: String,
        version: Option<ConfigIpVersion>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            url,
            version,
            poll_interval,
            current_ip: Arc::new(Mutex::new(None)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch current IP from HTTP service
    async fn fetch_ip(&self) -> Result<IpAddr> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| Error::provider("http", format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::provider(
                "http",
                format!("HTTP error: {}", response.status()),
            ));
        }

        let ip_text = response
            .text()
            .await
            .map_err(|e| Error::provider("http", format!("Failed to read response: {}", e)))?;

        let ip_text = ip_text.trim();

        // Parse IP address
        let ip: IpAddr = ip_text
            .parse()
            .map_err(|_| Error::provider("http", format!("Invalid IP address: {}", ip_text)))?;

        // Filter by IP version if specified
        if let Some(version) = self.version {
            match version {
                ConfigIpVersion::V4 => {
                    if !ip.is_ipv4() {
                        return Err(Error::provider(
                            "http",
                            format!("Expected IPv4, got: {}", ip),
                        ));
                    }
                }
                ConfigIpVersion::V6 => {
                    if !ip.is_ipv6() {
                        return Err(Error::provider(
                            "http",
                            format!("Expected IPv6, got: {}", ip),
                        ));
                    }
                }
                ConfigIpVersion::Both => {}
            }
        }

        Ok(ip)
    }
}

#[async_trait::async_trait]
impl IpSource for HttpIpSource {
    async fn current(&self) -> Result<IpAddr> {
        // Return cached IP if available and fresh (< 30 seconds old)
        // This reduces unnecessary HTTP requests
        if let Some(ip) = *self.current_ip.lock().await {
            // Cache is valid for 30 seconds
            return Ok(ip);
        }

        // Fetch fresh IP
        let ip = self.fetch_ip().await?;
        *self.current_ip.lock().await = Some(ip);
        Ok(ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let url = self.url.clone();
        let version_filter = self.version;
        let poll_interval = self.poll_interval;
        let current_ip = self.current_ip.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            tracing::info!(
                "Starting HTTP IP monitoring (url={}, interval={:?})",
                url,
                poll_interval
            );

            let mut last_known_ip: Option<IpAddr> = None;

            loop {
                // Fetch current IP
                match client.get(&url).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.text().await {
                                Ok(ip_text) => {
                                    let ip_text = ip_text.trim();
                                    match ip_text.parse::<IpAddr>() {
                                        Ok(ip) => {
                                            // Filter by version if specified
                                            let acceptable = if let Some(version) = version_filter {
                                                match version {
                                                    ConfigIpVersion::V4 => ip.is_ipv4(),
                                                    ConfigIpVersion::V6 => ip.is_ipv6(),
                                                    ConfigIpVersion::Both => true,
                                                }
                                            } else {
                                                true
                                            };

                                            if acceptable && last_known_ip != Some(ip) {
                                                tracing::info!(
                                                    "IP changed: {:?} -> {:?}",
                                                    last_known_ip,
                                                    ip
                                                );

                                                let event = IpChangeEvent::new(ip, last_known_ip);
                                                if tx.send(event).is_err() {
                                                    tracing::error!(
                                                        "Receiver dropped, stopping monitor"
                                                    );
                                                    break;
                                                }

                                                last_known_ip = Some(ip);
                                                *current_ip.lock().await = Some(ip);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Failed to parse IP address '{}': {}",
                                                ip_text,
                                                e
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to read response: {}", e);
                                }
                            }
                        } else {
                            tracing::warn!("HTTP error: {}", response.status());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("HTTP request failed: {}", e);
                    }
                }

                // Wait before next poll
                tokio::time::sleep(poll_interval).await;
            }
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }

    fn version(&self) -> Option<TraitsIpVersion> {
        match self.version {
            Some(ConfigIpVersion::V4) => Some(TraitsIpVersion::V4),
            Some(ConfigIpVersion::V6) => Some(TraitsIpVersion::V6),
            Some(ConfigIpVersion::Both) => None,
            None => None,
        }
    }
}

/// Factory for creating HTTP IP sources
pub struct HttpFactory;

impl IpSourceFactory for HttpFactory {
    fn create(&self, config: &IpSourceConfig) -> Result<Box<dyn IpSource>> {
        match config {
            IpSourceConfig::Http { url, interval_secs } => {
                let url = url.clone();
                let interval = Duration::from_secs(*interval_secs);

                Ok(Box::new(HttpIpSource::with_interval(
                    url, None, // version filtering can be added later
                    interval,
                )))
            }
            _ => Err(Error::config("Invalid config for HTTP IP source")),
        }
    }
}

/// Register the HTTP IP source with a registry
pub fn register(registry: &ProviderRegistry) {
    registry.register_ip_source("http", Box::new(HttpFactory));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creation() {
        let factory = HttpFactory;

        let config = IpSourceConfig::Http {
            url: "https://api.ipify.org".to_string(),
            interval_secs: 60,
        };

        let source = factory.create(&config);
        assert!(source.is_ok());
    }
}
