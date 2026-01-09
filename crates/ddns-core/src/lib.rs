// # ddns-core
//
// Core library for the event-driven DDNS system.
//
// ⚠️ ARCHITECTURAL CONSTRAINTS ⚠️
//
// This module is governed by .ai/AI_CONTRACT.md.
// Before modifying:
// 1. Read .ai/AI_CONTRACT.md
// 2. Understand the strict responsibility boundaries
// 3. Ensure changes don't violate core-first or plugin principles
//
// ## Architecture Overview
//
// This library provides the core functionality for dynamic DNS updates:
// - **IpSource**: Trait for detecting and monitoring IP changes
// - **DnsProvider**: Trait for updating DNS records via provider APIs
// - **StateStore**: Trait for persistent state management (idempotency)
// - **DdnsEngine**: Core engine that orchestrates the IP change → DNS update flow
// - **ProviderRegistry**: Plugin-based registry for DNS providers
//
// ## Design Principles
//
// 1. **Separation of Concerns**: Core logic is separate from implementations
// 2. **Event-Driven**: Uses async streams for IP change monitoring
// 3. **Plugin-Based**: Providers are registered dynamically, no hard-coded if-else
// 4. **Library-First**: All core functionality can be used as a library
// 5. **Idempotency**: State management ensures safe, repeatable operations

pub mod traits;
pub mod engine;
pub mod registry;
pub mod config;
pub mod error;
pub mod state;

// Re-export core types for convenience
pub use traits::{IpSource, DnsProvider, StateStore};
pub use engine::DdnsEngine;
pub use registry::ProviderRegistry;
pub use config::{DdnsConfig, IpSourceConfig, ProviderConfig};
pub use error::{Error, Result};
pub use state::{MemoryStateStore, FileStateStore};
