// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation Status
//
// **TEMPORARY**: Netlink implementation is reverted to skeleton.
// The rtnetlink crate API has compatibility issues between different
// environments (macOS development vs Linux CI/Alpine Docker).
//
// ## Future Implementation Path
//
// When implementing Netlink operations:
// 1. Use `netlink-sys` crate for direct Netlink socket access
// 2. Subscribe to RTMGRP_IPV4_IFADDR and RTMGRP_IPV6_IFADDR
// 3. Parse netlink packets with `netlink-packet-route`
// 4. Emit IpChangeEvent when address changes
// 5. Handle socket errors and reconnection
//
// ## Alternative: Use `neli` crate
//
// The `neli` crate provides a higher-level Netlink API that may have
// better cross-environment compatibility.
//
// ## Platform Support
//
// This crate only compiles on Linux due to Netlink being a Linux-specific feature.

use ddns_core::config::IpSourceConfig;

#[cfg(target_os = "linux")]
use ddns_core::config::IpVersion as ConfigIpVersion;

#[cfg(target_os = "linux")]
use ddns_core::traits::{IpSource, IpSourceFactory, IpVersion as TraitsIpVersion};

#[cfg(not(target_os = "linux"))]
use ddns_core::traits::{IpSource, IpSourceFactory};

use ddns_core::{Error, Result};
use ddns_core::ProviderRegistry;

#[cfg(target_os = "linux")]
use std::net::IpAddr;

#[cfg(target_os = "linux")]
use std::pin::Pin;

#[cfg(target_os = "linux")]
use tokio_stream::Stream;

/// Netlink-based IP source for Linux (Skeleton Implementation)
#[cfg(target_os = "linux")]
#[allow(dead_code)] // Fields reserved for future implementation
pub struct NetlinkIpSource {
    interface: Option<String>,
    version: Option<ConfigIpVersion>,
    current_ip: Option<IpAddr>,
}

#[cfg(target_os = "linux")]
impl NetlinkIpSource {
    pub fn new(interface: Option<String>, version: Option<ConfigIpVersion>) -> Self {
        Self {
            interface,
            version,
            current_ip: None,
        }
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        // TODO: Implement with netlink-sys or neli
        // For now, return error to force use of HTTP source
        Err(Error::not_found(
            "Netlink IP source not implemented yet. Use HTTP source instead.",
        ))
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = ddns_core::traits::IpChangeEvent> + Send + 'static>> {
        // TODO: Implement with netlink-sys or neli
        use tokio_stream::wrappers::UnboundedReceiverStream;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        drop(tx);
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
