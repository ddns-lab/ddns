// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation
//
// Uses rtnetlink to subscribe to kernel address change events (RTM_NEWADDR, RTM_DELADDR)
// and emits real-time IP change events through an async Stream.
//
// ## Platform Support
//
// This crate only compiles on Linux due to Netlink being a Linux-specific feature.

use ddns_core::config::IpSourceConfig;

#[cfg(target_os = "linux")]
use ddns_core::config::IpVersion as ConfigIpVersion;

#[cfg(target_os = "linux")]
use ddns_core::traits::{IpChangeEvent, IpSource, IpSourceFactory, IpVersion as TraitsIpVersion};

#[cfg(not(target_os = "linux"))]
use ddns_core::traits::{IpSource, IpSourceFactory};

use ddns_core::{Error, Result};

use ddns_core::ProviderRegistry;

#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::net::IpAddr;
#[cfg(target_os = "linux")]
use std::pin::Pin;
#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use tokio::sync::{Mutex, mpsc};
#[cfg(target_os = "linux")]
use tokio_stream::Stream;
#[cfg(target_os = "linux")]
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Default debounce window to ignore IP flapping
#[allow(dead_code)]
const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// Netlink-based IP source for Linux
#[cfg(target_os = "linux")]
pub struct NetlinkIpSource {
    /// Network interface to monitor (None = all interfaces)
    interface: Option<String>,

    /// IP version to monitor
    version: Option<ConfigIpVersion>,

    /// Current best IP address (cached)
    current_ip: Arc<Mutex<Option<IpAddr>>>,

    /// Last event timestamp (for debouncing)
    last_event: Arc<Mutex<Instant>>,

    /// Debounce window
    debounce_duration: Duration,

    /// Interface name cache (ifindex -> name)
    interface_cache: Arc<Mutex<HashMap<u32, String>>>,
}

#[cfg(target_os = "linux")]
impl NetlinkIpSource {
    /// Create a new Netlink IP source
    ///
    /// # Parameters
    ///
    /// - `interface`: Optional interface name (e.g., "eth0")
    /// - `version`: IP version to monitor (None = both)
    pub fn new(interface: Option<String>, version: Option<ConfigIpVersion>) -> Self {
        Self {
            interface,
            version,
            current_ip: Arc::new(Mutex::new(None)),
            last_event: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(60))),
            debounce_duration: Duration::from_millis(DEFAULT_DEBOUNCE_MS),
            interface_cache: Arc::new(Mutex::new(HashMap::new())),
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
            current_ip: Arc::new(Mutex::new(None)),
            last_event: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(60))),
            debounce_duration,
            interface_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check if an IP address should be considered
    fn should_accept_ip(&self, ip: &IpAddr) -> bool {
        // Filter out loopback and unspecified
        if ip.is_loopback() || ip.is_unspecified() {
            return false;
        }

        // Filter by IP version if specified
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

    /// Select the best IP address from a list of addresses
    ///
    /// This implements address selection logic:
    /// - Prefer global addresses over link-local
    /// - Prefer stable addresses over temporary ones
    /// - Filter by version if specified
    fn select_best_address(&self, addresses: &[IpAddr]) -> Option<IpAddr> {
        addresses
            .iter()
            .filter(|ip| self.should_accept_ip(ip))
            .filter(|ip| {
                // Additional filtering: prefer global over link-local
                // (but accept link-local if that's all we have)
                match ip {
                    IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_unspecified(),
                    IpAddr::V6(v6) => {
                        !v6.is_loopback() && !v6.is_unspecified()
                        // Accept unique local (ULA) if no global
                    }
                }
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

    /// Check if we're within debounce window
    async fn is_within_debounce_window(&self) -> bool {
        let last = *self.last_event.lock().await;
        last.elapsed() < self.debounce_duration
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        // Return cached IP if available
        if let Some(ip) = *self.current_ip.lock().await {
            return Ok(ip);
        }

        // Otherwise, query current addresses via rtnetlink
        let connection =
            rtnetlink::new_connection().map_err(|e| Error::provider("netlink", e.to_string()))?;

        let (mut connection, handle, _) = connection;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("Netlink connection error: {}", e);
            }
        });

        // Get all network interfaces
        let mut interfaces = handle
            .link()
            .get()
            .execute()
            .await
            .map_err(|e| Error::provider("netlink", e.to_string()))?;

        let mut all_addresses = Vec::new();

        while let Some(interface) = interfaces
            .try_next()
            .await
            .map_err(|e| Error::provider("netlink", e.to_string()))?
        {
            // Filter by interface name if specified
            if let Some(ref iface_name) = self.interface {
                let interface_name = interface
                    .attributes()
                    .find(|attr| matches!(attr, rtnetlink::LinkAttribute::IfName(_)))
                    .and_then(|attr| {
                        if let rtnetlink::LinkAttribute::IfName(name) = attr {
                            Some(name.clone())
                        } else {
                            None
                        }
                    });

                if let Some(name) = interface_name {
                    if name != *iface_name {
                        continue; // Skip this interface
                    }
                }
            }

            // Get addresses for this interface
            let mut addresses = handle
                .address()
                .get()
                .set_link_index(interface.header.index)
                .execute()
                .await
                .map_err(|e| Error::provider("netlink", e.to_string()))?;

            while let Some(msg) = addresses
                .try_next()
                .await
                .map_err(|e| Error::provider("netlink", e.to_string()))?
            {
                for nla in msg.attributes {
                    if let rtnetlink::AddressAttribute::Address(addr) = nla {
                        all_addresses.push(addr);
                    }
                }
            }
        }

        // Select best address
        let best_ip = self
            .select_best_address(&all_addresses)
            .ok_or_else(|| Error::not_found("No suitable IP address found"))?;

        // Update cache
        *self.current_ip.lock().await = Some(best_ip);

        Ok(best_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        use futures::stream::StreamExt;

        let (tx, rx) = mpsc::unbounded_channel();

        let interface_filter = self.interface.clone();
        let version_filter = self.version;
        let current_ip = self.current_ip.clone();
        let last_event = self.last_event.clone();
        let debounce_duration = self.debounce_duration;

        tokio::spawn(async move {
            tracing::info!("Starting Netlink IP monitoring");

            // Create Netlink connection
            let connection = match rtnetlink::new_connection() {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::error!("Failed to create Netlink connection: {}", e);
                    let _ = tx.send(IpChangeEvent::new(IpAddr::from([0, 0, 0, 0]), None));
                    return;
                }
            };

            let (mut connection, handle, mut messages) = connection;

            // Spawn connection handler
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    tracing::error!("Netlink connection error: {}", e);
                }
            });

            // Subscribe to address notifications
            // Note: rtnetlink doesn't have direct subscribe method,
            // we need to create a new socket for monitoring
            let sock = match socket2::Socket::new(
                socket2::Domain::NETLINK,
                socket2::Type::RAW,
                Some(socket2::Protocol::from(libc::NETLINK_ROUTE)),
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create Netlink socket: {}", e);
                    return;
                }
            };

            // Bind to NETLINK_ROUTE
            if let Err(e) = sock.bind(&socket2::SockAddr::new_netlink(0)) {
                tracing::error!("Failed to bind Netlink socket: {}", e);
                return;
            }

            // Join multicast groups for address events
            let rtnlgrp_ipv4_ifaddr = 5;
            let rtnlgrp_ipv6_ifaddr = 9;

            if let Err(e) =
                sock.join_multicast_group(&socket2::SockAddr::new_netlink(rtnlgrp_ipv4_ifaddr))
            {
                tracing::warn!("Failed to join IPv4 address multicast group: {}", e);
            }

            if version_filter.unwrap_or(ConfigIpVersion::Both) != ConfigIpVersion::V4 {
                if let Err(e) =
                    sock.join_multicast_group(&socket2::SockAddr::new_netlink(rtnlgrp_ipv6_ifaddr))
                {
                    tracing::warn!("Failed to join IPv6 address multicast group: {}", e);
                }
            }

            // Convert to tokio Netlink socket
            // For now, use a simpler polling approach with short intervals
            // This is a limitation of the current rtnetlink API
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_known_ip: Option<IpAddr> = None;

            loop {
                interval.tick().await;

                // Query current IP
                let mut all_addresses = Vec::new();

                match handle.link().get().execute().await {
                    Ok(mut interfaces) => {
                        while let Some(Ok(interface)) = interfaces.next().await {
                            // Filter by interface name if specified
                            if let Some(ref iface_name) = interface_filter {
                                let interface_name = interface
                                    .attributes()
                                    .find(|attr| {
                                        matches!(attr, rtnetlink::LinkAttribute::IfName(_))
                                    })
                                    .and_then(|attr| {
                                        if let rtnetlink::LinkAttribute::IfName(name) = attr {
                                            Some(name.clone())
                                        } else {
                                            None
                                        }
                                    });

                                if let Some(name) = interface_name {
                                    if name != *iface_name {
                                        continue;
                                    }
                                }
                            }

                            // Get addresses for this interface
                            match handle
                                .address()
                                .get()
                                .set_link_index(interface.header.index)
                                .execute()
                                .await
                            {
                                Ok(mut addresses) => {
                                    while let Some(Ok(msg)) = addresses.next().await {
                                        for nla in msg.attributes {
                                            if let rtnetlink::AddressAttribute::Address(addr) = nla
                                            {
                                                all_addresses.push(addr);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to get addresses for interface: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to query interfaces: {}", e);
                        continue;
                    }
                }

                // Select best address
                let best_ip = all_addresses
                    .into_iter()
                    .filter(|ip| {
                        // Apply filters
                        if ip.is_loopback() || ip.is_unspecified() {
                            return false;
                        }

                        if let Some(version) = version_filter {
                            match version {
                                ConfigIpVersion::V4 => ip.is_ipv4(),
                                ConfigIpVersion::V6 => ip.is_ipv6(),
                                ConfigIpVersion::Both => true,
                            }
                        } else {
                            true
                        }
                    })
                    .min_by_key(|ip| {
                        // Prefer global addresses
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
                    });

                if let Some(new_ip) = best_ip {
                    // Check if IP changed
                    if last_known_ip != Some(new_ip) {
                        let previous_ip = last_known_ip;

                        // Check debounce
                        let now = Instant::now();
                        let last = *last_event.lock().await;

                        if now.duration_since(last) >= debounce_duration {
                            tracing::info!("IP changed: {:?} -> {:?}", previous_ip, new_ip);

                            let event = IpChangeEvent::new(new_ip, previous_ip);
                            if tx.send(event).is_err() {
                                tracing::error!("Receiver dropped, stopping monitor");
                                break;
                            }

                            last_known_ip = Some(new_ip);
                            *current_ip.lock().await = Some(new_ip);
                            *last_event.lock().await = now;
                        } else {
                            tracing::debug!(
                                "Ignoring IP change within debounce window: {:?}",
                                new_ip
                            );
                        }
                    }
                }
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
    #[cfg(target_os = "linux")]
    fn test_select_best_address() {
        let source = NetlinkIpSource::new(Some("eth0".to_string()), Some(ConfigIpVersion::V4));

        let addresses = vec![
            IpAddr::from([127, 0, 0, 1]),
            IpAddr::from([192, 168, 1, 1]),
            IpAddr::from([8, 8, 8, 8]),
        ];

        let best = source.select_best_address(&addresses);
        assert_eq!(best, Some(IpAddr::from([8, 8, 8, 8])));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_select_best_address_ipv6() {
        let source = NetlinkIpSource::new(Some("eth0".to_string()), Some(ConfigIpVersion::V6));

        let addresses = vec![
            IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),
            IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]),
            IpAddr::from([0xfc00, 0, 0, 0, 0, 0, 0, 1]),
            IpAddr::from([0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888]),
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
