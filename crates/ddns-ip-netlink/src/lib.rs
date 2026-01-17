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
// This crate only works on Linux due to Netlink being a Linux-specific feature.
// On non-Linux platforms, the factory returns an error.

use ddns_core::config::IpSourceConfig;

#[cfg(target_os = "linux")]
pub use ddns_core::config::IpVersion as ConfigIpVersion;

#[cfg(target_os = "linux")]
use ddns_core::traits::{IpChangeEvent, IpSource, IpSourceFactory, IpVersion as TraitsIpVersion};

#[cfg(not(target_os = "linux"))]
use ddns_core::traits::{IpSource, IpSourceFactory};

use ddns_core::ProviderRegistry;
use ddns_core::{Error, Result};

#[cfg(target_os = "linux")]
use std::net::IpAddr;

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

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

    /// Check if an IP address is a public (routable) address
    fn is_public_ip(&self, ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                let octets = ipv4.octets();
                // Private IPv4 ranges:
                // 10.0.0.0/8 (10.0.0.0 - 10.255.255.255)
                // 172.16.0.0/12 (172.16.0.0 - 172.31.255.255)
                // 192.168.0.0/16 (192.168.0.0 - 192.168.255.255)
                // 169.254.0.0/16 (link-local)
                // 127.0.0.0/8 (loopback, already filtered)
                !(octets[0] == 10
                    || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
                    || (octets[0] == 192 && octets[1] == 168)
                    || (octets[0] == 169 && octets[1] == 254)
                    || octets[0] == 127)
            }
            IpAddr::V6(ipv6) => {
                let segments = ipv6.segments();
                // Private IPv6 ranges:
                // fc00::/7 (Unique Local Address - ULA)
                // fe80::/10 (link-local)
                // ff00::/8 (multicast)
                // ::1 (loopback, already filtered)
                // :: (unspecified, already filtered)
                !(segments[0] & 0xfe00 == 0xfc00
                    || (segments[0] & 0xffc0 == 0xfe80)
                    || (segments[0] & 0xff00 == 0xff00))
            }
        }
    }

    /// Query current IP addresses by reading from /proc/net/if_inet6
    fn query_addresses_proc(&self) -> Result<Vec<IpAddr>> {
        use std::fs::read_to_string;

        let content = read_to_string("/proc/net/if_inet6")
            .map_err(|_| Error::ip_source("Failed to read /proc/net/if_inet6"))?;

        let mut addresses = Vec::new();

        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }

            // Parse IPv6 address from /proc/net/if_inet6
            // Format: addr index prefix_len scope status iface_name
            let addr_hex = parts[0];
            let if_name = parts[5];

            // Filter by interface if specified
            if let Some(ref iface) = self.interface
                && iface != if_name
            {
                continue;
            }

            // Parse hex IPv6 address
            if addr_hex.len() == 32 {
                let mut addr_bytes = [0u8; 16];
                for (i, chunk) in (0..32).step_by(8).enumerate() {
                    let byte_val =
                        u32::from_str_radix(&addr_hex[chunk..chunk + 8], 16).unwrap_or(0);
                    addr_bytes[i * 4] = (byte_val >> 24) as u8;
                    addr_bytes[i * 4 + 1] = (byte_val >> 16) as u8;
                    addr_bytes[i * 4 + 2] = (byte_val >> 8) as u8;
                    addr_bytes[i * 4 + 3] = byte_val as u8;
                }

                let addr = IpAddr::from(addr_bytes);
                if self.should_accept_ip(&addr) {
                    addresses.push(addr);
                }
            }
        }

        // Also read IPv4 addresses from /proc/net/dev (parse via ifconfig or similar)
        // For simplicity, we'll use a different approach for IPv4
        self.query_ipv4_addresses(&mut addresses);

        if addresses.is_empty() {
            Err(Error::not_found("No suitable IP addresses found"))
        } else {
            Ok(addresses)
        }
    }

    /// Query IPv4 addresses using ioctl
    fn query_ipv4_addresses(&self, addresses: &mut Vec<IpAddr>) {
        unsafe {
            // For each interface, get IPv4 addresses using ioctl
            let interfaces_to_check: Vec<String> = if let Some(ref iface) = self.interface {
                vec![iface.clone()]
            } else {
                // Read all interfaces from /proc/net/dev
                match self.read_all_interfaces() {
                    Ok(ifaces) => ifaces,
                    Err(_) => return,
                }
            };

            for iface_name in interfaces_to_check {
                let mut ifreq: libc::ifreq = std::mem::zeroed();
                std::ptr::copy_nonoverlapping(
                    iface_name.as_bytes().as_ptr() as *const i8,
                    ifreq.ifr_name.as_mut_ptr(),
                    iface_name.len().min(libc::IFNAMSIZ - 1),
                );

                let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
                if sock < 0 {
                    continue;
                }

                // Get IPv4 address
                let ret = libc::ioctl(sock, libc::SIOCGIFADDR, &ifreq);
                libc::close(sock);

                if ret == 0 {
                    // Parse sockaddr
                    let addr = &ifreq.ifr_ifru.ifru_addr;
                    if addr.sa_family == libc::AF_INET as u16 {
                        let sin = &*(addr as *const libc::sockaddr as *const libc::sockaddr_in);
                        let ip = u32::from_be(sin.sin_addr.s_addr);
                        let ip_addr: IpAddr = std::net::Ipv4Addr::from(ip).into();
                        if self.should_accept_ip(&ip_addr) {
                            addresses.push(ip_addr);
                        }
                    }
                }
            }
        }
    }

    /// Read all interface names from /proc/net/dev
    fn read_all_interfaces(&self) -> Result<Vec<String>> {
        use std::fs::read_to_string;

        let content = read_to_string("/proc/net/dev")
            .map_err(|_| Error::ip_source("Failed to read /proc/net/dev"))?;

        let mut interfaces = Vec::new();

        // Skip header lines
        for line in content.lines().skip(2) {
            let line = line.trim();
            if let Some(colon_pos) = line.find(':') {
                let iface_name = line[..colon_pos].trim();
                if !iface_name.is_empty() {
                    interfaces.push(iface_name.to_string());
                }
            }
        }

        Ok(interfaces)
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        let addresses = self.query_addresses_proc()?;

        // Select IP based on version configuration
        let selected = match self.version {
            Some(ConfigIpVersion::V4) => {
                // IPv4 only: prefer public IP, fall back to any IPv4
                addresses
                    .iter()
                    .find(|ip| ip.is_ipv4() && self.is_public_ip(ip))
                    .or_else(|| addresses.iter().find(|ip| ip.is_ipv4()))
            }
            Some(ConfigIpVersion::V6) => {
                // IPv6 only: prefer public IP, fall back to any IPv6
                addresses
                    .iter()
                    .find(|ip| ip.is_ipv6() && self.is_public_ip(ip))
                    .or_else(|| addresses.iter().find(|ip| ip.is_ipv6()))
            }
            Some(ConfigIpVersion::Both) | None => {
                // Both versions: prefer public IPv4, then public IPv6, then any
                addresses
                    .iter()
                    .find(|ip| ip.is_ipv4() && self.is_public_ip(ip))
                    .or_else(|| {
                        addresses
                            .iter()
                            .find(|ip| ip.is_ipv6() && self.is_public_ip(ip))
                    })
                    .or_else(|| addresses.iter().find(|ip| ip.is_ipv4()))
                    .or_else(|| addresses.first())
            }
        };

        selected
            .copied()
            .ok_or_else(|| Error::not_found("No suitable IP addresses found"))
    }

    fn watch(&self) -> Pin<Box<dyn tokio_stream::Stream<Item = IpChangeEvent> + Send + 'static>> {
        use netlink_sys::TokioSocket;
        use tokio_stream::wrappers::UnboundedReceiverStream;

        let interface = self.interface.clone();
        let version = self.version;
        let debounce_duration = self.debounce_duration;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Spawn async task to handle netlink messages
        tokio::spawn(async move {
            // Create async netlink socket
            let mut sock: netlink_sys::TokioSocket = match TokioSocket::new() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create TokioSocket: {}", e);
                    return;
                }
            };

            // Bind to NETLINK_ROUTE with specific groups
            match sock.bind(|sock: &netlink_sys::TokioSocket| {
                // Create socket address for RTNETLINK
                use std::os::fd::AsRawFd;
                let fd = sock.as_raw_fd();

                unsafe {
                    let mut addr: libc::sockaddr_nl = std::mem::zeroed();
                    addr.nl_family = libc::AF_NETLINK as u16;
                    addr.nl_pid = std::process::id();
                    addr.nl_groups = (libc::RTMGRP_IPV4_IFADDR | libc::RTMGRP_IPV6_IFADDR) as u32;

                    let result = libc::bind(
                        fd,
                        &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                        std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
                    );

                    if result < 0 {
                        return Err(std::io::Error::last_os_error());
                    }

                    // Set receive buffer size
                    let bufsize: i32 = 8192 * 8;
                    libc::setsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_RCVBUF,
                        &bufsize as *const i32 as *const libc::c_void,
                        std::mem::size_of::<i32>() as libc::socklen_t,
                    );

                    Ok(())
                }
            }) {
                Ok(_) => tracing::info!("Netlink IP monitoring started (async)"),
                Err(e) => {
                    tracing::error!("Failed to bind TokioSocket: {}", e);
                    return;
                }
            }

            // Create a temporary source instance to query addresses
            let temp_source = NetlinkIpSource {
                interface: interface.clone(),
                version,
                debounce_duration,
            };

            // Track last known IPs separately for v4 and v6
            let mut last_v4: Option<IpAddr> = None;
            let mut last_v6: Option<IpAddr> = None;
            let mut last_event = tokio::time::Instant::now() - tokio::time::Duration::from_secs(60);

            // Get initial addresses
            if let Ok(addrs) = temp_source.query_addresses_proc() {
                last_v4 = match temp_source.version {
                    Some(ConfigIpVersion::V4) | Some(ConfigIpVersion::Both) | None => {
                        addrs
                            .iter()
                            .find(|ip| ip.is_ipv4() && temp_source.is_public_ip(ip))
                            .or_else(|| addrs.iter().find(|ip| ip.is_ipv4()))
                            .copied()
                    }
                    Some(ConfigIpVersion::V6) => None,
                };

                last_v6 = match temp_source.version {
                    Some(ConfigIpVersion::V6) | Some(ConfigIpVersion::Both) | None => {
                        addrs
                            .iter()
                            .find(|ip| ip.is_ipv6() && temp_source.is_public_ip(ip))
                            .or_else(|| addrs.iter().find(|ip| ip.is_ipv6()))
                            .copied()
                    }
                    Some(ConfigIpVersion::V4) => None,
                };

                tracing::info!("Initial IPs: v4={:?}, v6={:?}", last_v4, last_v6);
            }

            // Receive loop - use async recv
            let mut recv_buf = vec![0u8; 8192];

            loop {
                // Async receive
                match sock.recv(&mut recv_buf).await {
                    Ok(nread) => {
                        if nread == 0 {
                            tracing::warn!("Netlink socket received 0 bytes, closing");
                            break;
                        }

                        // Parse netlink message header
                        if nread >= 16 {
                            // nlmsghdr format: bytes 4-5 = nlmsg_type
                            let msg_type = u16::from_ne_bytes([recv_buf[4], recv_buf[5]]);

                            // Check if this is an address event
                            if msg_type == libc::RTM_NEWADDR as u16
                                || msg_type == libc::RTM_DELADDR as u16
                            {
                                let now = tokio::time::Instant::now();

                                // Apply debounce
                                if now.duration_since(last_event) > debounce_duration {
                                    // Re-query all IPs
                                    if let Ok(addrs) = temp_source.query_addresses_proc() {
                                        // Select IPv4 and IPv6 using the same logic as current()
                                        let new_v4 = match temp_source.version {
                                            Some(ConfigIpVersion::V4) | Some(ConfigIpVersion::Both) | None => {
                                                addrs
                                                    .iter()
                                                    .find(|ip| ip.is_ipv4() && temp_source.is_public_ip(ip))
                                                    .or_else(|| addrs.iter().find(|ip| ip.is_ipv4()))
                                                    .copied()
                                            }
                                            Some(ConfigIpVersion::V6) => None,
                                        };

                                        let new_v6 = match temp_source.version {
                                            Some(ConfigIpVersion::V6) | Some(ConfigIpVersion::Both) | None => {
                                                addrs
                                                    .iter()
                                                    .find(|ip| ip.is_ipv6() && temp_source.is_public_ip(ip))
                                                    .or_else(|| addrs.iter().find(|ip| ip.is_ipv6()))
                                                    .copied()
                                            }
                                            Some(ConfigIpVersion::V4) => None,
                                        };

                                        // Check for IPv4 changes
                                        if last_v4 != new_v4
                                            && let Some(v4) = new_v4
                                        {
                                            tracing::info!(
                                                "IPv4 changed: {:?} -> {:?}",
                                                last_v4,
                                                v4
                                            );
                                            let event = IpChangeEvent::new(v4, last_v4);
                                            if tx.send(event).is_err() {
                                                tracing::error!(
                                                    "Receiver dropped, stopping monitor"
                                                );
                                                break;
                                            }
                                            last_v4 = new_v4;
                                        }

                                        // Check for IPv6 changes
                                        if last_v6 != new_v6
                                            && let Some(v6) = new_v6
                                        {
                                            tracing::info!(
                                                "IPv6 changed: {:?} -> {:?}",
                                                last_v6,
                                                v6
                                            );
                                            let event = IpChangeEvent::new(v6, last_v6);
                                            if tx.send(event).is_err() {
                                                tracing::error!(
                                                    "Receiver dropped, stopping monitor"
                                                );
                                                break;
                                            }
                                            last_v6 = new_v6;
                                        }

                                        last_event = now;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Netlink recv error: {}", e);
                        break;
                    }
                }
            }

            tracing::info!("Netlink IP monitoring stopped");
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

    #[test]
    #[cfg(target_os = "linux")]
    fn test_should_accept_ip() {
        let source = NetlinkIpSource::new(None, Some(ConfigIpVersion::V4));

        // IPv4 addresses
        assert!(source.should_accept_ip(&"192.168.1.1".parse().unwrap()));
        assert!(source.should_accept_ip(&"8.8.8.8".parse().unwrap()));

        // IPv6 addresses
        assert!(!source.should_accept_ip(&"2001:db8::1".parse().unwrap()));

        // Special addresses
        assert!(!source.should_accept_ip(&"127.0.0.1".parse().unwrap())); // loopback
        assert!(!source.should_accept_ip(&"0.0.0.0".parse().unwrap())); // unspecified
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_should_accept_ip_v6() {
        let source = NetlinkIpSource::new(None, Some(ConfigIpVersion::V6));

        // IPv4 addresses
        assert!(!source.should_accept_ip(&"192.168.1.1".parse().unwrap()));

        // IPv6 addresses
        assert!(source.should_accept_ip(&"2001:db8::1".parse().unwrap()));
        assert!(source.should_accept_ip(&"2001:4860:4860::8888".parse().unwrap()));

        // Special addresses
        assert!(!source.should_accept_ip(&"::1".parse().unwrap())); // loopback
        assert!(!source.should_accept_ip(&"::".parse().unwrap())); // unspecified
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_debounce() {
        let source = NetlinkIpSource::with_debounce(None, None, Duration::from_millis(100));

        assert_eq!(source.debounce_duration, Duration::from_millis(100));
    }
}
