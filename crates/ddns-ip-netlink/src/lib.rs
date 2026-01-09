// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation
//
// Uses netlink-sys to subscribe to kernel address change events (RTM_NEWADDR, RTM_DELADDR)
// and emits real-time IP change events through an async Stream.
//
// This is a **true event-driven** implementation with no polling.
//
// ## Platform Support
//
// This crate only compiles on Linux due to Netlink being a Linux-specific feature.

use ddns_core::config::IpSourceConfig;

#[cfg(target_os = "linux")]
use ddns_core::traits::{IpChangeEvent, IpSource, IpSourceFactory, IpVersion as TraitsIpVersion};

#[cfg(not(target_os = "linux"))]
use ddns_core::traits::{IpSource, IpSourceFactory};

use ddns_core::ProviderRegistry;
use ddns_core::{Error, Result};

#[cfg(target_os = "linux")]
use std::net::IpAddr;

#[cfg(target_os = "linux")]
use std::pin::Pin;

#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

/// Default debounce window to ignore IP flapping
#[cfg(target_os = "linux")]
const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// Netlink-based IP source for Linux
#[cfg(target_os = "linux")]
pub struct NetlinkIpSource {
    /// Network interface to monitor (None = all interfaces)
    interface: Option<String>,

    /// IP version to monitor
    version: Option<ConfigIpVersion>,

    /// Current IP address (cached)
    current_ip: Option<IpAddr>,

    /// Last event timestamp (for debouncing)
    last_event: Instant,

    /// Debounce window
    debounce_duration: Duration,
}

#[cfg(target_os = "linux")]
impl NetlinkIpSource {
    /// Create a new Netlink IP source
    pub fn new(interface: Option<String>, version: Option<ConfigIpVersion>) -> Self {
        Self {
            interface,
            version,
            current_ip: None,
            last_event: Instant::now() - Duration::from_secs(60),
            debounce_duration: Duration::from_millis(DEFAULT_DEBOUNCE_MS),
        }
    }

    /// Create with custom debounce duration
    pub fn with_debounce(
        interface: Option<String>,
        version: Option<ConfigIpVersion>,
        debounce_duration: Duration,
    ) -> Self {
        Self {
            interface,
            version,
            current_ip: None,
            last_event: Instant::now() - Duration::from_secs(60),
            debounce_duration,
        }
    }

    /// Check if an IP address should be considered
    fn should_accept_ip(&self, ip: &IpAddr) -> bool {
        if ip.is_loopback() || ip.is_unspecified() {
            return false;
        }

        if let Some(version) = self.version {
            match version {
                ConfigIpVersion::V4 => ip.is_ipv4(),
                ConfigIpVersion::V6 => ip.is_ipv6(),
                ConfigIpVersion::Both => true,
            }
        } else {
            true
        }
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        // TODO: Implement using netlink-sys
        Err(Error::not_found("Netlink current() not yet implemented"))
    }

    fn watch(&self) -> Pin<Box<dyn tokio_stream::Stream<Item = IpChangeEvent> + Send + 'static>> {
        // TODO: Implement using netlink-sys with async monitoring
        use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            tracing::warn!("Netlink IP monitoring not yet implemented");
            drop(tx);
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

/// Factory for creating Netlink IP sources
pub struct NetlinkFactory;

#[cfg(target_os = "linux")]
impl IpSourceFactory for NetlinkFactory {
    fn create(&self, config: &IpSourceConfig) -> Result<Box<dyn IpSource>> {
        match config {
            IpSourceConfig::Netlink { interface, version } => {
                Ok(Box::new(NetlinkIpSource::new(interface.clone(), *version)))
            }
            _ => Err(Error::config("Invalid config for Netlink IP source")),
        }
    }
}

#[cfg(not(target_os = "linux"))]
impl IpSourceFactory for NetlinkFactory {
    fn create(&self, _config: &IpSourceConfig) -> Result<Box<dyn IpSource>> {
        Err(Error::config(
            "Netlink IP source is only supported on Linux",
        ))
    }
}

/// Register the Netlink IP source with a registry
pub fn register(registry: &ProviderRegistry) {
    registry.register_ip_source("netlink", Box::new(NetlinkFactory));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_factory_creation() {
        let factory = NetlinkFactory;

        let config = IpSourceConfig::Netlink {
            interface: Some("eth0".to_string()),
            version: Some(ConfigIpVersion::V4),
        };

        let source = factory.create(&config);
        assert!(source.is_ok());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_factory_unsupported() {
        let factory = NetlinkFactory;

        let config = IpSourceConfig::Netlink {
            interface: None,
            version: None,
        };

        let source = factory.create(&config);
        assert!(source.is_err());
    }
}
