//! Netlink VETH Test Program
//!
//! This is a standalone test program for real-environment netlink event testing.
//! It creates veth pairs for isolated testing (won't affect eth0 or other real interfaces).
//!
//! Usage:
//!   sudo ./netlink-veth-test
//!
//! Features:
//! - Creates veth-test0/veth-test1 pair (isolated from real network)
//! - Tests IPv4 address change events
//! - Tests IPv6 address change events
//! - Shows netlink events being received in real-time
//! - No risk to SSH connectivity (eth0 untouched)

use ddns_core::traits::ip_source::IpSource;
use ddns_ip_netlink::NetlinkIpSource;
use std::process::Command;
use std::time::Duration;
use tokio_stream::StreamExt;

const VETH_TEST: &str = "veth-test0";
const VETH_PEER: &str = "veth-test1";

/// Helper to create a veth pair
fn create_veth_pair(name: &str, peer: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating veth pair: {}/{}", name, peer);

    let status = Command::new("ip")
        .args(&["link", "add", name, "type", "veth", "peer", "name", peer])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to create veth pair {}/{}", name, peer).into());
    }

    // Bring both interfaces up
    let status = Command::new("ip")
        .args(&["link", "set", name, "up"])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to bring interface {} up", name).into());
    }

    let status = Command::new("ip")
        .args(&["link", "set", peer, "up"])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to bring interface {} up", peer).into());
    }

    println!("  ✓ veth pair created and brought up");
    Ok(())
}

/// Helper to delete a network interface
fn delete_interface(interface: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("ip")
        .args(&["link", "delete", interface])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to delete interface {}", interface).into());
    }

    Ok(())
}

/// Helper to set IP address on an interface
fn set_interface_ip(interface: &str, ip: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("ip")
        .args(&["addr", "add", ip, "dev", interface])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to set IP {} on interface {}", ip, interface).into());
    }

    Ok(())
}

/// Helper to flush all IPs from an interface
fn flush_interface_ips(interface: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("ip")
        .args(&["addr", "flush", "dev", interface])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to flush IPs from interface {}", interface).into());
    }

    Ok(())
}

/// Cleanup function
fn cleanup() {
    println!("\nCleaning up test interfaces...");
    let _ = delete_interface(VETH_TEST);
    let _ = delete_interface(VETH_PEER);
    println!("  ✓ Cleanup complete");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║     Netlink VETH Test - Real Environment Event Test      ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("This test will:");
    println!("  1. Create veth-test0/veth-test1 pair (isolated)");
    println!("  2. Test IPv4 address change events");
    println!("  3. Test IPv6 address change events");
    println!("  4. Verify netlink events are received");
    println!();
    println!("⚠️  Requires root privileges (CAP_NET_ADMIN)");
    println!("⚠️  Will NOT affect eth0 or other real interfaces");
    println!();

    // Cleanup any existing test interfaces first
    cleanup();

    // Setup: Create veth pair
    println!("\n═══════════════════════════════════════════════════════════");
    println!("SETUP: Creating veth pair");
    println!("═══════════════════════════════════════════════════════════");

    create_veth_pair(VETH_TEST, VETH_PEER)?;

    // Give interface time to stabilize
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Set initial IP
    println!("\nSetting initial IPv4 address: 192.168.99.1/24");
    set_interface_ip(VETH_TEST, "192.168.99.1/24")?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create netlink source
    println!("\n═══════════════════════════════════════════════════════════");
    println!("TEST: Creating netlink IP source");
    println!("═══════════════════════════════════════════════════════════");

    let source = NetlinkIpSource::new(Some(VETH_TEST.to_string()), None);

    // Test current() method
    println!("\nTesting current() method...");
    match source.current().await {
        Ok(ip) => println!("  ✓ current() returned: {}", ip),
        Err(e) => println!("  ✗ current() failed: {:?}", e),
    }

    // Start watch()
    println!("\n═══════════════════════════════════════════════════════════");
    println!("TEST: Starting netlink event watch");
    println!("═══════════════════════════════════════════════════════════");

    let mut stream = source.watch();

    // Give netlink thread time to start
    tokio::time::sleep(Duration::from_millis(300)).await;

    // TEST 1: IPv4 address change
    println!("\n═══════════════════════════════════════════════════════════");
    println!("TEST 1: IPv4 Address Change");
    println!("═══════════════════════════════════════════════════════════");

    println!("Flushing existing IPs and setting new IPv4: 192.168.99.2/24");
    flush_interface_ips(VETH_TEST)?;
    set_interface_ip(VETH_TEST, "192.168.99.2/24")?;

    println!("Waiting for IPv4 event...");
    match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
        Ok(Some(event)) => {
            println!("  ✓ IPv4 event received!");
            println!("    New IP: {}", event.new_ip);
            println!("    Old IP: {:?}", event.previous_ip);
        }
        Ok(None) => println!("  ✗ Stream ended"),
        Err(_) => println!("  ✗ Timeout waiting for IPv4 event"),
    }

    // TEST 2: IPv6 address change
    println!("\n═══════════════════════════════════════════════════════════");
    println!("TEST 2: IPv6 Address Change");
    println!("═══════════════════════════════════════════════════════════");

    println!("Flushing IPs and setting IPv6: 2001:db8::1/128");
    flush_interface_ips(VETH_TEST)?;
    set_interface_ip(VETH_TEST, "2001:db8::1/128")?;

    println!("Waiting for IPv6 event...");
    match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
        Ok(Some(event)) => {
            println!("  ✓ IPv6 event received!");
            println!("    New IP: {}", event.new_ip);
            println!("    Old IP: {:?}", event.previous_ip);
        }
        Ok(None) => println!("  ✗ Stream ended"),
        Err(_) => println!("  ✗ Timeout waiting for IPv6 event"),
    }

    // TEST 3: Rapid changes (debounce test)
    println!("\n═══════════════════════════════════════════════════════════");
    println!("TEST 3: Rapid IP Changes (Debounce)");
    println!("═══════════════════════════════════════════════════════════");

    println!("Applying 5 rapid IP changes (debounce window: 500ms)...");
    for i in 1..=5 {
        flush_interface_ips(VETH_TEST)?;
        let ip = format!("192.168.99.{}/24", i);
        set_interface_ip(VETH_TEST, &ip)?;
        println!("  Set: {}", ip);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Collect events for 2 seconds
    println!("Collecting events for 2 seconds...");
    let mut events = Vec::new();
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        match tokio::time::timeout(Duration::from_millis(500), stream.next()).await {
            Ok(Some(event)) => {
                println!("  Event received: {}", event.new_ip);
                events.push(event);
            }
            _ => break,
        }
    }

    println!("  Total events received: {} (debounced from 5 changes)", events.len());

    // Final summary
    println!("\n═══════════════════════════════════════════════════════════");
    println!("SUMMARY");
    println!("═══════════════════════════════════════════════════════════");

    if events.is_empty() {
        println!("⚠️  No events were received during the test.");
        println!("    This could indicate:");
        println!("    - Netlink socket not properly bound");
        println!("    - Missing CAP_NET_ADMIN capability");
        println!("    - Kernel not sending RTM_NEWADDR/RTM_DELADDR events");
    } else {
        println!("✓ Test completed successfully!");
        println!("  Netlink events are being received properly.");
        println!("  The system can detect IP changes and trigger DNS updates.");
    }

    // Cleanup
    cleanup();

    Ok(())
}
