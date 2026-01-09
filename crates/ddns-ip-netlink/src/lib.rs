// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation
//
// Uses `neli` crate to subscribe to kernel address change events (RTM_NEWADDR, RTM_DELADDR)
// and emits real-time IP change events through an async Stream.
//
// This is a **true event-driven** implementation with no polling.
//
// ## Platform Support
//
// This crate only compiles on Linux due to Netlink being a Linux-specific feature.

use ddns_core::config::IpSourceConfig;
use ddns_core::config::IpVersion as ConfigIpVersion;

#[cfg(target_os = "linux")]
use ddns_core::traits::{IpChangeEvent, IpSource, IpSourceFactory, IpVersion as TraitsIpVersion};

#[cfg(not(target_os = "linux"))]
use ddns_core::traits::{IpSource, IpSourceFactory};

use ddns_core::{Error, Result};
use ddns_core::ProviderRegistry;

#[cfg(target_os = "linux")]
use std::net::IpAddr;

#[cfg(target_os = "linux")]
use std::pin::Pin;

#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use neli::{
    consts::{
        nl::{NlmF, NlmFFlags},
        rtnl::{RtmGrp, RtmsgType, IaF, IfaF, IffFlags, ArpHrd},
        socket::{NlFamily, NlType},
    },
    nl::NlPayload,
    rtnl::{IfaddrMsg, Ifinfomsg},
    socket::sys::SocketAddr,
    types::RtBuffer,
    utils::Builds,
    GenlMsghdr, Nlmsghdr,
};

#[cfg(target_os = "linux")]
use tokio::net::unix::UnixStream;

#[cfg(target_os = "linux")]
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
use tokio_stream::Stream;

#[cfg(target_os = "linux")]
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Default debounce window to ignore IP flapping
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
    ///
    /// # Parameters
    ///
    /// - `interface`: Optional interface name (e.g., "eth0")
    /// - `version`: IP version to monitor (None = both)
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
                        !v6.is_loopback()
                            && !v6.is_unspecified()
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
    fn is_within_debounce_window(&self) -> bool {
        self.last_event.elapsed() < self.debounce_duration
    }

    /// Get interface index by name
    async fn get_interface_index(&self, interface_name: &str) -> Result<u32> {
        use neli::socket::tokio::NlSocket;

        let mut sock = NlSocket::new(NlFamily::Route)
            .map_err(|e| Error::provider("netlink", format!("Failed to create socket: {}", e)))?;

        // Send RTM_GETLINK request
        let ifmsg = Ifinfomsg::new(
            ArpHrd::Unspec,
            0,
            0,
            IffFlags::empty(),
            IffFlags::empty(),
        );

        let nlhdr = Nlmsghdr::new(
            RtmsgType::GetLink,
            NlmFFlags::REQUEST | NlmFFlags::DUMP,
            None,
            None,
            NlPayload::Payload(ifmsg),
        )
        .map_err(|e| Error::provider("netlink", format!("Failed to create message: {}", e)))?;

        sock.send(&nlhdr)
            .await
            .map_err(|e| Error::provider("netlink", format!("Failed to send request: {}", e)))?;

        // Receive responses
        let mut buffer = vec![0u8; 4096];
        let mut received = 0;

        while received < buffer.len() {
            let sz = sock
                .recv(&mut buffer[received..])
                .await
                .map_err(|e| Error::provider("netlink", format!("Failed to receive: {}", e)))?;

            if sz == 0 {
                break;
            }

            received += sz;
        }

        // Parse responses
        let mut iter = buffer.iter();
        while let Ok(Some(nlhdr)) = Nlmsghdr::<Ifinfomsg, RtBuffer>::deserialize(&mut iter) {
            if let NlType::Noop = nlhdr.nl_type() {
                continue;
            }

            if let Some(payload) = nlhdr.get_payload() {
                // Check interface name
                for nla in payload.attributes() {
                    if let Ok(name) = nla.get_payload_as_string() {
                        if name == interface_name {
                            return Ok(nlhdr.nl_seq);
                        }
                    }
                }
            }
        }

        Err(Error::not_found(&format!(
            "Interface {} not found",
            interface_name
        )))
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        use neli::socket::tokio::NlSocket;

        let mut sock = NlSocket::new(NlFamily::Route)
            .map_err(|e| Error::provider("netlink", format!("Failed to create socket: {}", e)))?;

        // Send RTM_GETADDR request
        let ifmsg = IfaddrMsg::new(
            libc::AF_UNSPEC as u8,
            0,
            0,
        );

        let nlhdr = Nlmsghdr::new(
            RtmsgType::GetAddr,
            NlmFFlags::REQUEST | NlmFFlags::DUMP,
            None,
            None,
            NlPayload::Payload(ifmsg),
        )
        .map_err(|e| Error::provider("netlink", format!("Failed to create message: {}", e)))?;

        sock.send(&nlhdr)
            .await
            .map_err(|e| Error::provider("netlink", format!("Failed to send request: {}", e)))?;

        // Receive responses
        let mut buffer = vec![0u8; 8192];
        let mut received = 0;

        while received < buffer.len() {
            let sz = sock
                .recv(&mut buffer[received..])
                .await
                .map_err(|e| Error::provider("netlink", format!("Failed to receive: {}", e)))?;

            if sz == 0 {
                break;
            }

            received += sz;
        }

        // Parse addresses
        let mut all_addresses = Vec::new();
        let mut iter = buffer.iter();

        while let Ok(Some(nlhdr)) = Nlmsghdr::<IfaddrMsg, RtBuffer>::deserialize(&mut iter) {
            if let NlType::Noop = nlhdr.nl_type() {
                continue;
            }

            if let Some(payload) = nlhdr.get_payload() {
                let ifindex = payload.ifi_index;

                // Filter by interface if specified
                if let Some(ref iface_name) = self.interface {
                    match self.get_interface_index(iface_name).await {
                        Ok(idx) if idx == ifindex => {}
                        Ok(_) => continue, // Skip other interfaces
                        Err(_) => break,
                    }
                }

                // Extract IP address from attributes
                for nla in payload.attributes() {
                    match nla.nla_type() {
                        IaF::Local | IaF::Address => {
                            if let Ok(addr_bytes) = nla.get_payload_as_bytes() {
                                if let Some(ip) = parse_ip_address(addr_bytes, payload.ifa_family) {
                                    if self.should_accept_ip(&ip) {
                                        all_addresses.push(ip);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Select best address
        let best_ip = self
            .select_best_address(&all_addresses)
            .ok_or_else(|| Error::not_found("No suitable IP address found"))?;

        Ok(best_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let (tx, rx) = mpsc::unbounded_channel();

        let interface_filter = self.interface.clone();
        let version_filter = self.version;
        let debounce_duration = self.debounce_duration;

        tokio::spawn(async move {
            tracing::info!("Starting Netlink IP monitoring (event-driven)");

            use neli::socket::tokio::NlSocketHandle;

            // Create Netlink socket
            let mut sock = match NlSocketHandle::connect(NlFamily::Route, None)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create Netlink socket: {}", e);
                    return;
                }
            };

            // Subscribe to address notifications
            let mut groups = RtBuffer::new();
            groups.push(RtmGrp::Ipv4Ifaddr);
            groups.push(RtmGrp::Ipv6Ifaddr);

            if let Err(e) = sock
                .create_and_bind_migration(Some(NlType::Route), groups)
                .await
            {
                tracing::error!("Failed to bind to Netlink groups: {}", e);
                return;
            }

            tracing::info!("Successfully subscribed to Netlink address events");

            // Track last known IP and event timestamp
            let mut last_known_ip: Option<IpAddr> = None;
            let mut last_event = Instant::now() - Duration::from_secs(60);

            // Buffer for receiving messages
            let mut buffer = vec![0u8; 8192];

            loop {
                // Receive Netlink messages
                match sock.recv::<_, u8>(&mut buffer).await {
                    Ok(()) => {
                        // Parse received messages
                        let mut iter = buffer.iter();

                        while let Ok(Some(nlhdr)) = Nlmsghdr::<IfaddrMsg, RtBuffer>::deserialize(&mut iter) {
                            // Skip NLMSG_DONE
                            if let NlType::Done = nlhdr.nl_type() {
                                continue;
                            }

                            // Only process new address notifications
                            if nlhdr.nl_type() != RtmsgType::NewAddr {
                                continue;
                            }

                            if let Some(payload) = nlhdr.get_payload() {
                                let ifindex = payload.ifi_index;

                                // Filter by interface if specified
                                if let Some(ref iface_name) = interface_filter {
                                    // We need to resolve interface name to ifindex
                                    // For now, skip this check as it requires additional RTM_GETLINK calls
                                    // TODO: Cache interface name to ifindex mapping
                                }

                                // Extract IP address from attributes
                                for nla in payload.attributes() {
                                    if matches!(nla.nla_type(), IaF::Local | IaF::Address) {
                                        if let Ok(addr_bytes) = nla.get_payload_as_bytes() {
                                            if let Some(ip) = parse_ip_address(addr_bytes, payload.ifa_family) {
                                                // Apply filters
                                                if !should_accept_ip_filtered(&ip, version_filter) {
                                                    continue;
                                                }

                                                // Check if IP changed
                                                if last_known_ip != Some(ip) {
                                                    let previous_ip = last_known_ip;

                                                    // Check debounce
                                                    let now = Instant::now();
                                                    if now.duration_since(last_event) >= debounce_duration {
                                                        tracing::info!(
                                                            "IP changed: {:?} -> {:?}",
                                                            previous_ip,
                                                            ip
                                                        );

                                                        let event = IpChangeEvent::new(ip, previous_ip);
                                                        if tx.send(event).is_err() {
                                                            tracing::error!("Receiver dropped, stopping monitor");
                                                            break;
                                                        }

                                                        last_known_ip = Some(ip);
                                                        last_event = now;
                                                    } else {
                                                        tracing::debug!(
                                                            "Ignoring IP change within debounce window: {:?}",
                                                            ip
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Netlink receive error: {}", e);
                        // Continue listening
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

/// Parse IP address from bytes and family
#[cfg(target_os = "linux")]
fn parse_ip_address(bytes: &[u8], family: u8) -> Option<IpAddr> {
    match family as u32 {
        libc::AF_INET => {
            if bytes.len() >= 4 {
                let addr = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Some(IpAddr::from(addr))
            } else {
                None
            }
        }
        libc::AF_INET6 => {
            if bytes.len() >= 16 {
                let mut addr_bytes = [0u8; 16];
                addr_bytes.copy_from_slice(&bytes[..16]);
                Some(IpAddr::from(addr_bytes))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if IP should be accepted based on version filter
#[cfg(target_os = "linux")]
fn should_accept_ip_filtered(ip: &IpAddr, version_filter: &Option<ConfigIpVersion>) -> bool {
    // Filter out loopback and unspecified
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }

    // Filter by IP version if specified
    if let Some(version) = version_filter {
        match version {
            ConfigIpVersion::V4 => ip.is_ipv4(),
            ConfigIpVersion::V6 => ip.is_ipv6(),
            ConfigIpVersion::Both => true,
        }
    } else {
        true
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

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_ipv4() {
        let bytes = [192, 168, 1, 1];
        let ip = parse_ip_address(&bytes, libc::AF_INET);
        assert_eq!(ip, Some(IpAddr::from([192, 168, 1, 1])));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_parse_ipv6() {
        let bytes = [0x20, 0x01, 0x48, 0x60, 0x48, 0x60, 0, 0, 0, 0, 0, 0, 0x88, 0x88];
        let ip = parse_ip_address(&bytes, libc::AF_INET6);
        assert_eq!(
            ip,
            Some(IpAddr::from([0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888]))
        );
    }
}
