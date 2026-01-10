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
use ddns_core::config::IpVersion as ConfigIpVersion;

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

    /// Get the interface index by name
    fn get_interface_index(&self, name: &str) -> Result<u32> {
        use netlink_packet_route::RtnlMessage;
        use netlink_packet_route::interface::InterfaceMessage;
        use netlink_sys::Socket;

        let mut sock = Socket::new(netlink_sys::Protocol::Route)?;

        // Send RTM_GETLINK request to get interface index
        let mut nl_msg =
            netlink_packet_core::NetlinkMessage::new(netlink_packet_core::NetlinkPayload::from(
                RtnlMessage::GetLink(InterfaceMessage::default()),
            ));

        nl_msg.header.flags = netlink_packet_core::NlFFlags::new(&[
            netlink_packet_core::NlF::REQUEST,
            netlink_packet_core::NlF::DUMP,
        ]);

        let mut buf = vec![0u8; nl_msg.buffer_len()];
        nl_msg.serialize(&mut buf);
        sock.send(&buf, 0)?;

        // Receive and parse responses
        let mut recv_buf = vec![0u8; 8192];
        let mut interface_index = None;

        loop {
            let nread = sock.recv(&mut recv_buf, 0)?;
            if nread == 0 {
                break;
            }

            let bytes = &recv_buf[..nread];
            let mut offset = 0;

            while offset < bytes.len() {
                let bytes_slice = &bytes[offset..];
                match netlink_packet_core::NetlinkMessage::deserialize(bytes_slice) {
                    Ok(packet) => {
                        let packet_len = packet.header.length as usize;
                        if let netlink_packet_core::NetlinkPayload::InnerMessage(
                            RtnlMessage::NewLink(if_msg),
                        ) = packet.payload
                        {
                            if let Some(attr) = if_msg.attributes.nla() {
                                // Check interface name
                                for nla in attr.iter() {
                                    if let netlink_packet_route::InterfaceAttribute::IfName(
                                        ifname,
                                    ) = nla
                                    {
                                        if ifname == name {
                                            // Found the interface
                                            interface_index = Some(if_msg.header.index);
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        // Move to next message
                        if packet_len == 0 {
                            break;
                        }
                        offset += packet_len;

                        // Check if we found the interface
                        if interface_index.is_some() {
                            break;
                        }

                        // Check if we've processed all messages
                        if offset >= bytes.len() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Check if we found the interface
            if interface_index.is_some() {
                break;
            }
        }

        interface_index.ok_or_else(|| Error::not_found(&format!("Interface '{}' not found", name)))
    }

    /// Query current IP addresses from the kernel
    fn query_addresses(&self) -> Result<Vec<IpAddr>> {
        use netlink_packet_route::RtnlMessage;
        use netlink_packet_route::address::AddressMessage;
        use netlink_sys::Socket;

        let mut sock = Socket::new(netlink_sys::Protocol::Route)?;

        // Send RTM_GETADDR request
        let mut nl_msg =
            netlink_packet_core::NetlinkMessage::new(netlink_packet_core::NetlinkPayload::from(
                RtnlMessage::GetAddress(AddressMessage::default()),
            ));

        nl_msg.header.flags = netlink_packet_core::NlFFlags::new(&[
            netlink_packet_core::NlF::REQUEST,
            netlink_packet_core::NlF::DUMP,
        ]);

        let mut buf = vec![0u8; nl_msg.buffer_len()];
        nl_msg.serialize(&mut buf);
        sock.send(&buf, 0)?;

        // Receive and parse responses
        let mut recv_buf = vec![0u8; 8192];
        let mut addresses = Vec::new();

        loop {
            let nread = sock.recv(&mut recv_buf, 0)?;
            if nread == 0 {
                break;
            }

            let bytes = &recv_buf[..nread];

            match netlink_packet_core::NetlinkMessage::deserialize(bytes) {
                Ok(packet) => {
                    if let netlink_packet_core::NetlinkPayload::InnerMessage(
                        RtnlMessage::NewAddress(addr_msg),
                    ) = packet.payload
                    {
                        // Filter by interface if specified
                        if let Some(ref iface_name) = self.interface {
                            // Get interface index
                            let if_index = self.get_interface_index(iface_name)?;
                            if addr_msg.header.index != if_index {
                                continue;
                            }
                        }

                        // Extract IP address from attributes
                        if let Some(attrs) = addr_msg.attributes.nla() {
                            for nla in attrs.iter() {
                                match nla {
                                    netlink_packet_route::AddressAttribute::Local(ip)
                                    | netlink_packet_route::AddressAttribute::Address(ip) => {
                                        if self.should_accept_ip(&ip) {
                                            addresses.push(ip);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    // Check if this is the last message
                    if packet
                        .header
                        .flags
                        .contains(&netlink_packet_core::NlF::DUMP)
                    {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Failed to parse netlink message: {}", e);
                    break;
                }
            }
        }

        if addresses.is_empty() {
            Err(Error::not_found("No suitable IP addresses found"))
        } else {
            Ok(addresses)
        }
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl IpSource for NetlinkIpSource {
    async fn current(&self) -> Result<IpAddr> {
        let addresses = self.query_addresses()?;

        // Prefer IPv4 over IPv6 if both are available
        let addr = addresses
            .iter()
            .find(|ip| ip.is_ipv4())
            .or_else(|| addresses.first())
            .ok_or_else(|| Error::not_found("No IP addresses found"))?;

        Ok(*addr)
    }

    fn watch(&self) -> Pin<Box<dyn tokio_stream::Stream<Item = IpChangeEvent> + Send + 'static>> {
        use libc::{SO_RCVBUF, SOL_SOCKET, c_int, socklen_t};
        use netlink_sys::{Socket, SocketAddr};
        use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

        let interface = self.interface.clone();
        let version = self.version;
        let debounce_duration = self.debounce_duration;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        std::thread::spawn(move || {
            // Create Netlink socket
            let sock = match Socket::new(netlink_sys::Protocol::Route) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create netlink socket: {}", e);
                    drop(tx);
                    return;
                }
            };

            // Set receive buffer size
            let fd = sock.as_raw_fd();
            let bufsize: i32 = 8192 * 8;
            unsafe {
                libc::setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_RCVBUF,
                    &bufsize as *const i32 as *const libc::c_void,
                    std::mem::size_of::<i32>() as socklen_t,
                );
            }

            // Bind to address
            let addr = SocketAddr::new(0, libc::RTMGRP_IPV4_IFADDR | libc::RTMGRP_IPV6_IFADDR);
            if let Err(e) = sock.bind(&addr) {
                tracing::error!("Failed to bind netlink socket: {}", e);
                drop(tx);
                return;
            }

            tracing::info!("Netlink IP monitoring started");

            let mut last_ip = None;
            let mut last_event = Instant::now() - Duration::from_secs(60);

            // Receive loop
            let mut recv_buf = vec![0u8; 8192];

            loop {
                match sock.recv(&mut recv_buf, 0) {
                    Ok(nread) => {
                        if nread == 0 {
                            break;
                        }

                        let bytes = &recv_buf[..nread];

                        match netlink_packet_core::NetlinkMessage::deserialize(bytes) {
                            Ok(packet) => {
                                if let netlink_packet_core::NetlinkPayload::InnerMessage(
                                    RtnlMessage::NewAddress(addr_msg),
                                ) = packet.payload
                                {
                                    // Extract IP address
                                    let mut current_ips = Vec::new();

                                    if let Some(attrs) = addr_msg.attributes.nla() {
                                        for nla in attrs.iter() {
                                            match nla {
                                                netlink_packet_route::AddressAttribute::Local(
                                                    ip,
                                                )
                                                | netlink_packet_route::AddressAttribute::Address(
                                                    ip,
                                                ) => {
                                                    // Apply IP version filter
                                                    if let Some(v) = version {
                                                        match v {
                                                            ConfigIpVersion::V4 => {
                                                                if ip.is_ipv4()
                                                                    && !ip.is_loopback()
                                                                    && !ip.is_unspecified()
                                                                {
                                                                    current_ips.push(ip);
                                                                }
                                                            }
                                                            ConfigIpVersion::V6 => {
                                                                if ip.is_ipv6()
                                                                    && !ip.is_loopback()
                                                                    && !ip.is_unspecified()
                                                                {
                                                                    current_ips.push(ip);
                                                                }
                                                            }
                                                            ConfigIpVersion::Both => {
                                                                if !ip.is_loopback()
                                                                    && !ip.is_unspecified()
                                                                {
                                                                    current_ips.push(ip);
                                                                }
                                                            }
                                                        }
                                                    } else if !ip.is_loopback()
                                                        && !ip.is_unspecified()
                                                    {
                                                        current_ips.push(ip);
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }

                                    // Prefer IPv4
                                    let new_ip = current_ips
                                        .iter()
                                        .find(|ip| ip.is_ipv4())
                                        .or_else(|| current_ips.first())
                                        .copied();

                                    // Check if IP changed
                                    if new_ip != last_ip {
                                        let now = Instant::now();

                                        // Apply debounce
                                        if now.duration_since(last_event) > debounce_duration {
                                            if let Some(ip) = new_ip {
                                                if let Err(e) = tx.send(IpChangeEvent { ip }) {
                                                    tracing::error!(
                                                        "Failed to send IP change event: {}",
                                                        e
                                                    );
                                                    break;
                                                }

                                                tracing::info!("IP changed to {}", ip);
                                                last_ip = new_ip;
                                                last_event = now;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::debug!("Failed to parse netlink message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Netlink recv error: {}", e);
                        break;
                    }
                }

                // Small sleep to prevent tight loop on errors
                std::thread::sleep(Duration::from_millis(100));
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
        assert!(source.should_accept_ip(&"::1".parse().unwrap()));

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
