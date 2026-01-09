// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation
//
// Uses neli to subscribe to kernel address change events (RTM_NEWADDR, RTM_DELADDR)
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
        use neli::socket::NlSocket;
        use neli::nl::{Nlmsghdr, NlPayload, NlType};
        use neli::rtnl::Ifaddrmsg;
        use neli::consts::nl::{NlmF, NlmFFlags};
        use neli::consts::rtnl::{RtmType, RtAddrFamily, IfaF};
        use neli::types::Buffer;

        let mut sock = NlSocket::new(neli::consts::socket::NlFamily::Route, None, None)
            .map_err(|e| Error::provider("netlink", format!("Failed to create socket: {}", e)))?;

        // Create RTM_GETADDR request
        let ifmsg = Ifaddrmsg::new(RtAddrFamily::Unspecified, 0, 0);

        let nlhdr = Nlmsghdr::new(
            RtmType::GetAddr,
            NlmFFlags::new(&[NlmF::Request, NlmF::Dump]),
            None,
            None,
            NlPayload::Payload(ifmsg),
        )
        .map_err(|e| Error::provider("netlink", format!("Failed to create message: {}", e)))?;

        sock.send(&nlhdr)
            .map_err(|e| Error::provider("netlink", format!("Failed to send: {}", e)))?;

        // Receive responses
        let mut buffer = vec![0u8; 8192];
        let mut all_addresses = Vec::new();

        loop {
            let n = sock.recv(&mut buffer)
                .map_err(|e| Error::provider("netlink", format!("Failed to receive: {}", e)))?;

            if n == 0 {
                break;
            }

            // Parse responses
            let mut iter = buffer[..n].iter();

            while let Ok(Some(nlhdr)) = Nlmsghdr::<Ifaddrmsg, Buffer>::deserialize(&mut iter) {
                if nlhdr.nl_type() == RtmType::Done {
                    break;
                }

                if nlhdr.nl_type() == RtmType::NewAddr {
                    if let Some(payload) = nlhdr.get_payload() {
                        let family = payload.ifa_family;

                        for nla in payload.attributes() {
                            if matches!(nla.nla_type(), IfaF::Local | IfaF::Address) {
                                if let Ok(addr_bytes) = nla.get_payload_as_bytes() {
                                    if let Some(ip) = parse_ip_address(addr_bytes, family) {
                                        if self.should_accept_ip(&ip) {
                                            all_addresses.push(ip);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if iter.as_slice().is_empty() {
                break;
            }
        }

        let best_ip = all_addresses
            .first()
            .ok_or_else(|| Error::not_found("No suitable IP address found"))?;

        Ok(*best_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        use neli::socket::NlSocketHandle;
        use neli::consts::socket::NlFamily;
        use neli::consts::rtnl::{RtmGrp, RtmType, IfaF};

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let interface_filter = self.interface.clone();
        let version_filter = self.version;
        let debounce_duration = self.debounce_duration;

        tokio::spawn(async move {
            tracing::info!("Starting Netlink IP monitoring (event-driven)");

            // Create Netlink socket
            let mut sock = match NlSocketHandle::connect(NlFamily::Route, None).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create Netlink socket: {}", e);
                    return;
                }
            };

            // Subscribe to address notifications
            let mut groups = Vec::new();
            groups.push(RtmGrp::Ipv4Ifaddr);
            groups.push(RtmGrp::Ipv6Ifaddr);

            if let Err(e) = sock.create_and_bind_migration(None, groups).await {
                tracing::error!("Failed to bind to Netlink groups: {}", e);
                return;
            }

            tracing::info!("Successfully subscribed to Netlink address events");

            let mut last_known_ip: Option<IpAddr> = None;
            let mut last_event = Instant::now() - Duration::from_secs(60);

            let mut buffer = vec![0u8; 8192];

            loop {
                match sock.recv::<u8>(&mut buffer).await {
                    Ok(()) => {
                        let mut iter = buffer.iter();

                        while let Ok(Some(nlhdr)) = neli::nl::Nlmsghdr::<neli::rtnl::Ifaddrmsg, u8>::deserialize(&mut iter) {
                            if nlhdr.nl_type() == RtmType::NewAddr {
                                if let Some(payload) = nlhdr.get_payload() {
                                    let family = payload.ifa_family;

                                    for nla in payload.attributes() {
                                        if matches!(nla.nla_type(), IfaF::Local | IfaF::Address) {
                                            if let Ok(addr_bytes) = nla.get_payload_as_bytes() {
                                                if let Some(ip) = parse_ip_address(addr_bytes, family) {
                                                    if !should_accept_ip_filtered(&ip, version_filter) {
                                                        continue;
                                                    }

                                                    if last_known_ip != Some(ip) {
                                                        let previous_ip = last_known_ip;
                                                        let now = Instant::now();

                                                        if now.duration_since(last_event) >= debounce_duration {
                                                            tracing::info!("IP changed: {:?} -> {:?}", previous_ip, ip);

                                                            let event = IpChangeEvent::new(ip, previous_ip);
                                                            if tx.send(event).is_err() {
                                                                tracing::error!("Receiver dropped, stopping monitor");
                                                                break;
                                                            }

                                                            last_known_ip = Some(ip);
                                                            last_event = now;
                                                        } else {
                                                            tracing::debug!("Ignoring IP change within debounce window: {:?}", ip);
                                                        }
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
                    }
                }
            }
        });

        Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
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
fn parse_ip_address(bytes: &[u8], family: neli::consts::rtnl::RtAddrFamily) -> Option<IpAddr> {
    match family {
        neli::consts::rtnl::RtAddrFamily::Inet => {
            if bytes.len() >= 4 {
                let mut addr_bytes = [0u8; 4];
                addr_bytes.copy_from_slice(&bytes[..4]);
                Some(IpAddr::from(addr_bytes))
            } else {
                None
            }
        }
        neli::consts::rtnl::RtAddrFamily::Inet6 => {
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
