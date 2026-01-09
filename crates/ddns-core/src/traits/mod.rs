//! Core traits for the DDNS system
//!
//! This module defines the abstract interfaces that all implementations must follow.
//!
//! - [`IpSource`]: Monitor IP address changes
//! - [`DnsProvider`]: Update DNS records via provider APIs
//! - [`StateStore`]: Persistent state management for idempotency

pub mod dns_provider;
pub mod ip_source;
pub mod state_store;

pub use dns_provider::{DnsProvider, DnsProviderFactory, RecordMetadata, UpdateResult};
pub use ip_source::{IpChangeEvent, IpSource, IpSourceFactory, IpVersion};
pub use state_store::{StateRecord, StateStore, StateStoreFactory};
