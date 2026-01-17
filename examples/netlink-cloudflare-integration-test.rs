//! Netlink + Cloudflare Integration Test
//!
//! This test verifies that netlink IP changes properly trigger Cloudflare DNS updates.
//!
//! Usage:
//!   DDNS_DOMAIN=visional.cn \
//!   DDNS_SUBDOMAIN=ddns-test \
//!   CLOUDFLARE_API_TOKEN=your_token \
//!   CLOUDFLARE_ZONE_ID=94c68064f71931be238e9752b1b37af5 \
//!   cargo run --example netlink-cloudflare-integration-test --features cloudflare

use ddns_core::traits::{DnsProvider, IpSource};
use ddns_ip_netlink::NetlinkIpSource;
use ddns_provider_cloudflare::CloudflareProvider;
use std::env;
use std::time::Duration;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║   Netlink + Cloudflare Integration Test                     ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Read environment variables
    let api_token = env::var("CLOUDFLARE_API_TOKEN").unwrap_or_else(|_| {
        eprintln!("Error: CLOUDFLARE_API_TOKEN environment variable is required");
        std::process::exit(1);
    });

    let zone_id = env::var("CLOUDFLARE_ZONE_ID").unwrap_or_else(|_| {
        eprintln!("Error: CLOUDFLARE_ZONE_ID environment variable is required");
        std::process::exit(1);
    });

    let domain = env::var("DDNS_DOMAIN").unwrap_or_else(|_| {
        eprintln!("Error: DDNS_DOMAIN environment variable is required");
        std::process::exit(1);
    });

    let subdomain = env::var("DDNS_SUBDOMAIN").unwrap_or_else(|_| {
        eprintln!("Error: DDNS_SUBDOMAIN environment variable is required");
        std::process::exit(1);
    });

    let interface = env::var("DDNS_INTERFACE").ok();

    let record_name = if subdomain.is_empty() {
        domain.clone()
    } else {
        format!("{}.{}", subdomain, domain)
    };

    println!("Configuration:");
    println!("  Domain: {}", domain);
    println!("  Subdomain: {}", subdomain);
    println!("  Record: {}", record_name);
    println!("  Zone ID: {}", zone_id);
    println!("  Interface: {:?}", interface);
    println!("  Dry-run: false (LIVE UPDATES WILL BE MADE)");
    println!();
    println!("⚠️  WARNING: This will make REAL changes to your DNS records!");
    println!("    Press Ctrl+C to abort if this is not intended.");
    println!();

    // Create Cloudflare provider (live mode, no dry-run)
    let provider = CloudflareProvider::new(
        api_token,
        Some(zone_id),
        None, // account_id
        false, // dry_run = false (LIVE MODE)
    );

    println!("✓ Cloudflare provider created");
    println!();

    // Create netlink IP source
    println!("Creating Netlink IP source...");
    let ip_source = NetlinkIpSource::new(interface, None);

    println!("✓ Netlink IP source created");
    println!();

    // Get current IP
    println!("═══════════════════════════════════════════════════════════");
    println!("STEP 1: Get Current IP");
    println!("═══════════════════════════════════════════════════════════");

    // Try to get current IP (may fail if no public IP exists yet)
    let _current_ip = match ip_source.current().await {
        Ok(ip) => {
            println!("✓ Current IP: {}", ip);
            println!("  Type: {}", if ip.is_ipv4() { "IPv4" } else { "IPv6" });

            // Test initial DNS update
            println!();
            println!("═══════════════════════════════════════════════════════════");
            println!("STEP 2: Initial DNS Update (Current IP)");
            println!("═══════════════════════════════════════════════════════════");

            println!("Updating DNS record {} -> {}", record_name, ip);
            match provider.update_record(&record_name, ip).await {
                Ok(result) => {
                    match result {
                        ddns_core::traits::UpdateResult::Updated { previous_ip, new_ip } => {
                            println!("✓ DNS updated successfully!");
                            if let Some(prev) = previous_ip {
                                println!("  Previous IP: {}", prev);
                            }
                            println!("  New IP: {}", new_ip);
                        }
                        ddns_core::traits::UpdateResult::Unchanged { current_ip: curr_ip } => {
                            println!("✓ DNS already up to date");
                            println!("  Current IP: {}", curr_ip);
                        }
                        ddns_core::traits::UpdateResult::Created { new_ip } => {
                            println!("✓ DNS record created!");
                            println!("  IP: {}", new_ip);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Failed to update DNS: {:?}", e);
                    println!("  Check your API token and zone ID");
                }
            }

            Some(ip)
        }
        Err(e) => {
            println!("✗ No public IP available: {:?}", e);
            println!("  Will wait for IP changes via netlink...");
            println!("  (Add a public IP to any interface to trigger an update)");
            None
        }
    };

    println!();

    // Start watching for IP changes
    println!("═══════════════════════════════════════════════════════════");
    println!("STEP 3: Watch for IP Changes");
    println!("═══════════════════════════════════════════════════════════");

    println!("Listening for IP changes via netlink...");
    println!("Any PUBLIC IP change will trigger a DNS update to {}",
             record_name);
    println!();
    println!("Expected behavior:");
    println!("  - Private IP changes: LOGGED ONLY (no DNS update)");
    println!("  - Public IP changes: LOGGED + DNS UPDATE");
    println!();
    println!("To test, you can:");
    println!("  1. Add/remove public IPs on network interfaces");
    println!("  2. Use veth pairs for isolated testing");
    println!("  3. Wait for actual IP changes on your server");
    println!();
    println!("Press Ctrl+C to stop");
    println!();

    let mut stream = ip_source.watch();

    // Process events
    while let Some(event) = stream.next().await {
        println!("═══════════════════════════════════════════════════════════");
        println!("IP CHANGE EVENT RECEIVED");
        println!("═══════════════════════════════════════════════════════════");
        println!("New IP: {}", event.new_ip);
        println!("Previous IP: {:?}", event.previous_ip);
        println!("Version: {:?}", event.version);
        println!();

        // Update DNS
        println!("Updating DNS record...");
        match provider.update_record(&record_name, event.new_ip).await {
            Ok(result) => {
                match result {
                    ddns_core::traits::UpdateResult::Updated { previous_ip, new_ip } => {
                        println!("✓ DNS updated successfully!");
                        if let Some(prev) = previous_ip {
                            println!("  Previous DNS IP: {}", prev);
                        }
                        println!("  New DNS IP: {}", new_ip);
                    }
                    ddns_core::traits::UpdateResult::Unchanged { current_ip: ip } => {
                        println!("✓ DNS already up to date");
                        println!("  Current DNS IP: {}", ip);
                    }
                    ddns_core::traits::UpdateResult::Created { new_ip } => {
                        println!("✓ DNS record created!");
                        println!("  IP: {}", new_ip);
                    }
                }
            }
            Err(e) => {
                println!("✗ Failed to update DNS: {:?}", e);
            }
        }
        println!();
        println!("Waiting for next IP change...");
    }

    println!("Stream ended");
    Ok(())
}
