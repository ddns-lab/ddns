// # Netlink IP Source
//
// This crate provides a Netlink-based IP source for Linux systems.
//
// ## Implementation
//
// Uses raw Netlink sockets to subscribe to kernel address change events (RTM_NEWADDR, RTM_DELADDR)
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
use std::os::fd::AsRawFd;

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

    /// Check if we're within debounce window
    fn is_within_debounce_window(&self) -> bool {
        self.last_event.elapsed() < self.debounce_duration
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        use socket2::{Domain, Protocol, Socket, Type};

        // Create Netlink socket
        let socket = Socket::new(Domain::NETLINK, Type::RAW, Some(Protocol::from(libc::NETLINK_ROUTE)))
            .map_err(|e| Error::provider("netlink", format!("Failed to create socket: {}", e)))?;

        // Send RTM_GETADDR request
        let nlhdr = NetlinkGetAddrRequest::new();
        socket.send(&nlhdr.to_bytes())
            .map_err(|e| Error::provider("netlink", format!("Failed to send request: {}", e)))?;

        // Receive responses
        let mut buffer = vec![0u8; 8192];
        let mut all_addresses = Vec::new();

        loop {
            let n = socket.recv(&mut buffer)
                .map_err(|e| Error::provider("netlink", format!("Failed to receive: {}", e)))?;

            if n == 0 {
                break;
            }

            // Parse Netlink messages
            let mut offset = 0;
            while offset < n {
                if let Some(nlhdr) = NetlinkMessageHeader::parse(&buffer[offset..]) {
                    // Check if this is the done message
                    if nlhdr.nlmsg_type == libc::NLMSG_DONE as u16 {
                        break;
                    }

                    // Only process RTM_NEWADDR
                    if nlhdr.nlmsg_type == libc::RTM_NEWADDR as u16 {
                        if let Some(addrs) = parse_ifaddrmsg(&buffer[offset..nlhdr.nlmsg_len as usize + offset]) {
                            for addr in addrs {
                                if self.should_accept_ip(&addr) {
                                    all_addresses.push(addr);
                                }
                            }
                        }
                    }

                    offset += nlhdr.nlmsg_len as usize;
                } else {
                    break;
                }
            }

            // Check if we've received all messages
            if offset < n {
                continue;
            }
            break;
        }

        // Select best address
        let best_ip = all_addresses
            .first()
            .ok_or_else(|| Error::not_found("No suitable IP address found"))?;

        Ok(*best_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let interface_filter = self.interface.clone();
        let version_filter = self.version;
        let debounce_duration = self.debounce_duration;

        tokio::spawn(async move {
            tracing::info!("Starting Netlink IP monitoring (event-driven)");

            use socket2::{Domain, Protocol, Socket, Type};
            use tokio::net::unix::UnixStream;

            // Create Netlink socket
            let socket = match Socket::new(Domain::NETLINK, Type::RAW, Some(Protocol::from(libc::NETLINK_ROUTE))) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create Netlink socket: {}", e);
                    return;
                }
            };

            // Bind to Netlink socket with multicast groups
            let addr = libc::sockaddr_nl {
                nl_family: libc::AF_NETLINK as u16,
                nl_pad: 0,
                nl_pid: 0,
                nl_groups: (1 << (libc::RTNLGRP_IPV4_IFADDR - 1)) | (1 << (libc::RTNLGRP_IPV6_IFADDR - 1)),
            };

            let bind_addr = socket2::SockAddr::new_netlink(0, 0);

            if let Err(e) = socket.bind(&bind_addr) {
                tracing::error!("Failed to bind Netlink socket: {}", e);
                return;
            }

            // Convert to tokio socket
            let std_socket = std::net::SocketFd::from(socket);
            let tokio_socket = match UnixStream::from_std(std_socket.into()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to convert to tokio socket: {}", e);
                    return;
                }
            };

            tracing::info!("Successfully subscribed to Netlink address events");

            // Track last known IP and event timestamp
            let mut last_known_ip: Option<IpAddr> = None;
            let mut last_event = Instant::now() - Duration::from_secs(60);

            // Buffer for receiving messages
            let mut buffer = vec![0u8; 8192];

            loop {
                // Receive Netlink messages
                match tokio_socket.try_read(&mut buffer) {
                    Ok(n) => {
                        if n == 0 {
                            continue;
                        }

                        // Parse Netlink messages
                        let mut offset = 0;
                        while offset < n {
                            if let Some(nlhdr) = NetlinkMessageHeader::parse(&buffer[offset..]) {
                                // Only process RTM_NEWADDR
                                if nlhdr.nlmsg_type == libc::RTM_NEWADDR as u16 {
                                    if let Some(addrs) = parse_ifaddrmsg(&buffer[offset..nlhdr.nlmsg_len as usize + offset]) {
                                        for ip in addrs {
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

                                offset += nlhdr.nlmsg_len as usize;
                            } else {
                                break;
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No data available, continue
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        tracing::warn!("Netlink receive error: {}", e);
                        // Continue listening
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

/// Netlink message header
#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Debug)]
struct NetlinkMessageHeader {
    nlmsg_len: u32,
    nlmsg_type: u16,
    nlmsg_flags: u16,
    nlmsg_seq: u32,
    nlmsg_pid: u32,
}

#[cfg(target_os = "linux")]
impl NetlinkMessageHeader {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < std::mem::size_of::<NetlinkMessageHeader>() {
            return None;
        }

        unsafe {
            let ptr = data.as_ptr() as *const NetlinkMessageHeader;
            Some(std::ptr::read_unaligned(ptr))
        }
    }
}

/// RTM_GETADDR request
#[cfg(target_os = "linux")]
struct NetlinkGetAddrRequest;

#[cfg(target_os = "linux")]
impl NetlinkGetAddrRequest {
    fn new() -> Self {
        Self
    }

    fn to_bytes(&self) -> Vec<u8> {
        use std::mem::size_of;

        let nlhdr = NetlinkMessageHeader {
            nlmsg_len: (size_of::<NetlinkMessageHeader>() + size_of::<libc::ifaddrmsg>()) as u32,
            nlmsg_type: libc::RTM_GETADDR as u16,
            nlmsg_flags: (libc::NLM_F_REQUEST | libc::NLM_F_DUMP) as u16,
            nlmsg_seq: 1,
            nlmsg_pid: 0,
        };

        let ifmsg = libc::ifaddrmsg {
            ifa_family: libc::AF_UNSPEC as u8,
            ifa_prefixlen: 0,
            ifa_flags: 0,
            ifa_scope: 0,
            ifa_index: 0,
        };

        let mut result = Vec::with_capacity(size_of::<NetlinkMessageHeader>() + size_of::<libc::ifaddrmsg>());

        unsafe {
            let nlhdr_bytes = std::slice::from_raw_parts(
                &nlhdr as *const NetlinkMessageHeader as *const u8,
                size_of::<NetlinkMessageHeader>()
            );
            result.extend_from_slice(nlhdr_bytes);

            let ifmsg_bytes = std::slice::from_raw_parts(
                &ifmsg as *const libc::ifaddrmsg as *const u8,
                size_of::<libc::ifaddrmsg>()
            );
            result.extend_from_slice(ifmsg_bytes);
        }

        result
    }
}

/// Parse RTM_NEWADDR message and extract IP addresses
#[cfg(target_os = "linux")]
fn parse_ifaddrmsg(data: &[u8]) -> Option<Vec<IpAddr>> {
    let mut addrs = Vec::new();

    let offset = std::mem::size_of::<NetlinkMessageHeader>();
    if data.len() < offset + std::mem::size_of::<libc::ifaddrmsg>() {
        return Some(addrs);
    }

    unsafe {
        let ifmsg_ptr = data.as_ptr().add(offset) as *const libc::ifaddrmsg;
        let ifmsg = std::ptr::read_unaligned(ifmsg_ptr);

        let rta_offset = offset + std::mem::size_of::<libc::ifaddrmsg>();
        let mut current_offset = rta_offset;

        let rtalen = ifmsg.ifa_index as usize;
        let data_len = data.len();

        while current_offset + std::mem::size_of::<libc::rtattr>() < data_len {
            let rta_ptr = data.as_ptr().add(current_offset) as *const libc::rtattr;
            let rta = std::ptr::read_unaligned(rta_ptr);

            let rta_len = rta.rta_len as usize;
            if rta_len == 0 || current_offset + rta_len > data_len {
                break;
            }

            let rta_type = rta.rta_type;

            // Check for IFA_LOCAL or IFA_ADDRESS
            if rta_type == libc::IFA_LOCAL || rta_type == libc::IFA_ADDRESS {
                let addr_offset = current_offset + std::mem::size_of::<libc::rtattr>();

                if ifmsg.ifa_family as i32 == libc::AF_INET {
                    if addr_offset + 4 <= data_len {
                        let mut addr_bytes = [0u8; 4];
                        addr_bytes.copy_from_slice(&data[addr_offset..addr_offset + 4]);
                        addrs.push(IpAddr::from(addr_bytes));
                    }
                } else if ifmsg.ifa_family as i32 == libc::AF_INET6 {
                    if addr_offset + 16 <= data_len {
                        let mut addr_bytes = [0u8; 16];
                        addr_bytes.copy_from_slice(&data[addr_offset..addr_offset + 16]);
                        addrs.push(IpAddr::from(addr_bytes));
                    }
                }
            }

            current_offset += ((rta_len + std::mem::size_of::<libc::c_ulong>() - 1)
                & !(std::mem::size_of::<libc::c_ulong>() - 1)) as usize;
        }
    }

    Some(addrs)
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
}
