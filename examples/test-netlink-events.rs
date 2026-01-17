//! Simple test to verify netlink event reception
//!
//! Usage:
//!   RUST_LOG=debug cargo run --example test-netlink-events

use ddns_core::traits::IpSource;
use ddns_ip_netlink::NetlinkIpSource;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║         Netlink Event Stream Test                             ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    println!("Creating netlink IP source (all interfaces, all versions)...");
    let source = NetlinkIpSource::new(None, None);
    let mut stream = source.watch();

    println!("✓ Netlink event stream started");
    println!();
    println!("Instructions:");
    println!("  1. Watch this terminal for events");
    println!("  2. In another terminal, run:");
    println!("     ip link add veth-test type veth peer name veth-peer");
    println!("     ip link set veth-test up");
    println!("     ip addr add 198.51.100.1/32 dev veth-test");
    println!("     ip addr del 198.51.100.1/32 dev veth-test");
    println!("     ip addr add 203.0.113.1/32 dev veth-test");
    println!();
    println!("  Expected behavior:");
    println!("    - Private IP changes: Logged but NO event sent");
    println!("    - Public IP changes: Logged AND event sent");
    println!();

    let start = std::time::Instant::now();
    let mut event_count = 0;
    let timeout_duration = std::time::Duration::from_secs(60);

    while start.elapsed() < timeout_duration {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stream.next()
        ).await {
            Ok(Some(event)) => {
                event_count += 1;
                println!("═══════════════════════════════════════════════════════════");
                println!("EVENT #{} RECEIVED", event_count);
                println!("═══════════════════════════════════════════════════════════");
                println!("  New IP:      {}", event.new_ip);
                println!("  Previous IP: {:?}", event.previous_ip);
                println!("  Version:     {:?}", event.version);
                println!();
            }
            Ok(None) => {
                println!("Stream ended");
                break;
            }
            Err(_) => {
                // Timeout, continue waiting
            }
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("TEST SUMMARY");
    println!("═══════════════════════════════════════════════════════════");
    println!("Total events received: {}", event_count);
    println!();

    Ok(())
}
