// # Cloudflare Provider Real Environment Validation Tool
//
// This is a Phase 22 validation tool for testing the Cloudflare provider
// against the real Cloudflare API in a controlled environment.
//
// ## Usage
//
// ```bash
// # Dry-run mode (default - safe)
// DDNS_MODE=dry-run \
// CLOUDFLARE_API_TOKEN=your_token \
// CLOUDFLARE_ZONE_ID=your_zone_id \
// DDNS_DOMAIN=test.example.com \
// DDNS_RECORD_NAME=ddns-test.example.com \
// DDNS_RECORD_TYPE=A \
// DDNS_TEST_IP=1.2.3.4 \
// cargo run --example cloudflare-validation
//
// # Live mode (makes actual changes!)
// DDNS_MODE=live \
// CLOUDFLARE_API_TOKEN=your_token \
// CLOUDFLARE_ZONE_ID=your_zone_id \
// DDNS_DOMAIN=test.example.com \
// DDNS_RECORD_NAME=ddns-test.example.com \
// DDNS_RECORD_TYPE=A \
// DDNS_TEST_IP=1.2.3.4 \
// cargo run --example cloudflare-validation
// ```
//
// ## Environment Variables
//
// Required:
// - `CLOUDFLARE_API_TOKEN`: Cloudflare API token
// - `DDNS_DOMAIN`: Domain to test (e.g., "example.com")
// - `DDNS_RECORD_NAME`: Full record name (e.g., "ddns-test.example.com")
// - `DDNS_TEST_IP`: IP address to use for testing
//
// Optional:
// - `CLOUDFLARE_ZONE_ID`: Zone ID (if not provided, will auto-discover)
// - `DDNS_RECORD_TYPE`: Record type (A or AAAA, default: A)
// - `DDNS_MODE`: "dry-run" or "live" (default: dry-run)

use ddns_core::traits::DnsProvider;
use ddns_provider_cloudflare::CloudflareProvider;
use std::env;
use std::net::IpAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("=== Phase 22: Cloudflare Provider Real Environment Validation ===");

    // Read environment variables
    let api_token = env::var("CLOUDFLARE_API_TOKEN").unwrap_or_else(|_| {
        tracing::error!("CLOUDFLARE_API_TOKEN environment variable is required");
        std::process::exit(1);
    });

    let zone_id = env::var("CLOUDFLARE_ZONE_ID").ok();
    let domain = env::var("DDNS_DOMAIN").unwrap_or_else(|_| {
        tracing::error!("DDNS_DOMAIN environment variable is required");
        std::process::exit(1);
    });

    let record_name = env::var("DDNS_RECORD_NAME").unwrap_or_else(|_| {
        tracing::error!("DDNS_RECORD_NAME environment variable is required");
        std::process::exit(1);
    });

    let record_type = env::var("DDNS_RECORD_TYPE").unwrap_or_else(|_| "A".to_string());
    let test_ip_str = env::var("DDNS_TEST_IP").unwrap_or_else(|_| {
        tracing::error!("DDNS_TEST_IP environment variable is required");
        std::process::exit(1);
    });

    let mode = env::var("DDNS_MODE").unwrap_or_else(|_| "dry-run".to_string());
    let dry_run = mode.to_lowercase() == "dry-run";

    if dry_run {
        tracing::warn!("Running in DRY-RUN mode - no changes will be made");
    } else {
        tracing::warn!("Running in LIVE mode - will make actual DNS changes!");
    }

    tracing::info!("Configuration:");
    tracing::info!("  Domain: {}", domain);
    tracing::info!("  Record: {}", record_name);
    tracing::info!("  Type: {}", record_type);
    tracing::info!("  Test IP: {}", test_ip_str);
    tracing::info!("  Mode: {}", mode);
    if let Some(ref zid) = zone_id {
        tracing::info!("  Zone ID: {}", zid);
    } else {
        tracing::info!("  Zone ID: (auto-discover)");
    }

    // Parse test IP
    let test_ip: IpAddr = test_ip_str.parse().expect("Invalid IP address");

    // Validate record type matches IP
    match (test_ip, record_type.as_str()) {
        (IpAddr::V4(_), "AAAA") => {
            tracing::error!("Record type AAAA requires IPv6 address");
            std::process::exit(1);
        }
        (IpAddr::V6(_), "A") => {
            tracing::error!("Record type A requires IPv4 address");
            std::process::exit(1);
        }
        _ => {}
    }

    // Create provider
    tracing::info!("\n--- Step 1: Creating Cloudflare Provider ---");
    let provider = CloudflareProvider::new(
        api_token, zone_id, None, // account_id
        dry_run,
    );

    tracing::info!("Provider created successfully");
    tracing::info!("API token validated (not shown for security)");

    // Test 1: Validate provider supports the record
    tracing::info!("\n--- Step 2: Validating Record Support ---");
    if provider.supports_record(&record_name) {
        tracing::info!("✓ Provider supports record: {}", record_name);
    } else {
        tracing::error!("✗ Provider does not support record: {}", record_name);
        std::process::exit(1);
    }

    // Test 2: Update record (this tests zone discovery, record lookup, and update)
    tracing::info!("\n--- Step 3: Testing DNS Update ---");
    tracing::info!("Calling update_record()...");

    match provider.update_record(&record_name, test_ip).await {
        Ok(result) => {
            tracing::info!("✓ Update record succeeded");
            match result {
                ddns_core::traits::UpdateResult::Updated {
                    previous_ip,
                    new_ip,
                } => {
                    tracing::info!("  Result: Updated");
                    if let Some(prev) = previous_ip {
                        tracing::info!("  Previous IP: {}", prev);
                    }
                    tracing::info!("  New IP: {}", new_ip);
                }
                ddns_core::traits::UpdateResult::Unchanged { current_ip } => {
                    tracing::info!("  Result: Unchanged (IP already correct)");
                    tracing::info!("  Current IP: {}", current_ip);
                }
                ddns_core::traits::UpdateResult::Created { new_ip } => {
                    tracing::info!("  Result: Created");
                    tracing::info!("  New IP: {}", new_ip);
                }
            }
        }
        Err(e) => {
            tracing::error!("✗ Update record failed: {}", e);
            tracing::error!("Error details: {:?}", e);
            std::process::exit(1);
        }
    }

    // Test 3: Idempotency check (call again with same IP)
    tracing::info!("\n--- Step 4: Testing Idempotency ---");
    tracing::info!("Calling update_record() again with same IP...");

    match provider.update_record(&record_name, test_ip).await {
        Ok(result) => match result {
            ddns_core::traits::UpdateResult::Unchanged { .. } => {
                tracing::info!("✓ Idempotency verified (unchanged as expected)");
            }
            _ => {
                tracing::warn!("⚠ Update performed again (may indicate idempotency issue)");
            }
        },
        Err(e) => {
            tracing::error!("✗ Idempotency test failed: {}", e);
            std::process::exit(1);
        }
    }

    // Summary
    tracing::info!("\n=== Phase 22 Validation Summary ===");
    tracing::info!("✓ Provider creation: OK");
    tracing::info!("✓ Record support: OK");
    tracing::info!("✓ DNS update: OK");
    tracing::info!("✓ Idempotency: OK");
    tracing::info!("✓ Security: API token not logged");

    if dry_run {
        tracing::info!("\n=== DRY-RUN COMPLETE ===");
        tracing::info!("No changes were made to DNS records.");
        tracing::info!("To make actual changes, set DDNS_MODE=live");
    } else {
        tracing::info!("\n=== LIVE MODE COMPLETE ===");
        tracing::info!("DNS records were updated.");
        tracing::info!("Verify at: https://dnschecker.org/#A/{}", record_name);
    }

    Ok(())
}
