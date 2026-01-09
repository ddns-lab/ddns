//! Core traits for the DDNS system
//!
//! This module defines the abstract interfaces that all implementations must follow.
//!
//! - [`IpSource`]: Monitor IP address changes
//! - [`DnsProvider`]: Update DNS records via provider APIs
//! - [`StateStore`]: Persistent state management for idempotency

pub mod ip_source;
pub mod dns_provider;
pub mod state_store;

pub use ip_source::{IpSource, IpChangeEvent, IpVersion, IpSourceFactory};
pub use dns_provider::{DnsProvider, UpdateResult, RecordMetadata, DnsProviderFactory};
pub use state_store::{StateStore, StateRecord, StateStoreFactory};
