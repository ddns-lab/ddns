// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation Status
//
// **NOTE: This is a skeleton implementation.** The actual Netlink socket
// operations are not yet implemented. This crate defines the structure and
// trait implementations that will be filled in when the specific Netlink
// logic is added.
//
// ## Future Implementation
//
// When implementing the actual Netlink operations:
// 1. Add `netlink-sys`, `netlink-packet-route`, `futures` dependencies
// 2. Create Netlink socket and bind to RTMGRP_IPV4_ROUTE | RTMGRP_IPV6_ROUTE
// 3. Parse Netlink messages for address updates
// 4. Filter by interface name if specified
// 5. Emit IpChangeEvent when address changes
// 6. Handle socket errors and reconnection
//
// ## Netlink Reference
//
// - Netlink: https://man7.org/linux/man-pages/man7/netlink.7.html
// - RTNLink: https://man7.org/linux/man-pages/man7/rtnetlink.7.html
//
// ## Platform Support
//
// This crate only compiles on Linux due to Netlink being a Linux-specific feature.

use ddns_core::config::IpSourceConfig;
use ddns_core::traits::{IpSource, IpSourceFactory};
use ddns_core::{Error, Result};

#[cfg(target_os = "linux")]
use ddns_core::config::IpVersion;

#[cfg(target_os = "linux")]
use ddns_core::traits::IpChangeEvent;

#[cfg(target_os = "linux")]
use std::net::IpAddr;

#[cfg(target_os = "linux")]
use std::pin::Pin;

#[cfg(target_os = "linux")]
use tokio_stream::Stream;

/// Netlink-based IP source for Linux
#[cfg(target_os = "linux")]
pub struct NetlinkIpSource {
    /// Network interface to monitor (None = all interfaces)
    interface: Option<String>,

    /// IP version to monitor
    version: Option<IpVersion>,

    /// Current IP address (cached)
    current_ip: Option<IpAddr>,
}

#[cfg(target_os = "linux")]
impl NetlinkIpSource {
    /// Create a new Netlink IP source
    ///
    /// # Parameters
    ///
    /// - `interface`: Optional interface name (e.g., "eth0")
    /// - `version`: IP version to monitor (None = both)
    pub fn new(interface: Option<String>, version: Option<IpVersion>) -> Self {
        Self {
            interface,
            version,
            current_ip: None,
        }
    }

    /// Get the preferred IP address from a list of addresses
    ///
    /// This implements basic address selection logic:
    /// - Prefer global addresses over link-local
    /// - Prefer stable addresses over temporary ones
    /// - Filter by version if specified
    ///
    /// # Parameters
    ///
    /// - `addresses`: List of IP addresses
    ///
    /// # Returns
    ///
    /// The best IP address, or None if no suitable address found
    fn select_best_address(&self, addresses: &[IpAddr]) -> Option<IpAddr> {
        addresses
            .iter()
            .filter(|ip| {
                // Filter by IP version if specified
                if let Some(version) = self.version {
                    match version {
                        IpVersion::V4 => ip.is_ipv4(),
                        IpVersion::V6 => ip.is_ipv6(),
                    }
                } else {
                    true
                }
            })
            .filter(|ip| {
                // Filter out link-local and loopback addresses
                // (unless they're the only addresses available)
                !ip.is_loopback() && !ip.is_unspecified()
            })
            .min_by_key(|ip| {
                // Prefer global addresses (lower score = better)
                match ip {
                    IpAddr::V4(v4) => {
                        if v4.is_private() {
                            1
                        } else {
                            0
                        }
                    }
                    IpAddr::V6(v6) => {
                        if v6.is_loopback() {
                            3
                        } else if v6.is_unique_local() {
                            2
                        } else {
                            0
                        }
                    }
                }
            })
            .copied()
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        // TODO: Implement actual Netlink query
        // 1. Send RTM_GETADDR message
        // 2. Parse response to get interface addresses
        // 3. Select best address based on criteria
        // 4. Cache the result

        Err(Error::not_found("Netlink IP source not implemented"))
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        // TODO: Implement actual Netlink monitoring
        // 1. Create Netlink socket
        // 2. Subscribe to RTNLGRP_IPV4_ROUTE and/or RTNLGRP_IPV6_ROUTE
        // 3. Parse incoming messages for address changes
        // 4. Emit IpChangeEvent when address changes
        // 5. Handle socket errors and reconnection

        use tokio_stream::wrappers::UnboundedReceiverStream;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Placeholder: immediately close the stream
        drop(tx);

        Box::pin(UnboundedReceiverStream::new(rx))
    }

    fn version(&self) -> Option<IpVersion> {
        self.version
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
///
/// This function should be called during initialization to make the
/// Netlink IP source available.
///
/// # Example
///
/// ```rust
/// use ddns_core::ProviderRegistry;
///
/// let mut registry = ProviderRegistry::new();
/// ddns_ip_netlink::register(&registry);
/// ```
pub fn register(registry: &ddns_core::ProviderRegistry) {
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
            version: Some(IpVersion::V4),
        };

        let source = factory.create(&config);
        assert!(source.is_ok());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_select_best_address() {
        let source = NetlinkIpSource::new(Some("eth0".to_string()), Some(IpVersion::V4));

        let addresses = vec![
            IpAddr::from([127, 0, 0, 1]),   // loopback
            IpAddr::from([192, 168, 1, 1]), // private
            IpAddr::from([8, 8, 8, 8]),     // public
        ];

        let best = source.select_best_address(&addresses);
        assert_eq!(best, Some(IpAddr::from([8, 8, 8, 8])));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_select_best_address_ipv6() {
        let source = NetlinkIpSource::new(Some("eth0".to_string()), Some(IpVersion::V6));

        let addresses = vec![
            IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),      // loopback
            IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]), // link-local
            IpAddr::from([0xfc00, 0, 0, 0, 0, 0, 0, 1]), // ULA
            IpAddr::from([0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888]), // global
        ];

        let best = source.select_best_address(&addresses);
        assert_eq!(
            best,
            Some(IpAddr::from([0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888]))
        );
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
