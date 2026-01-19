//! Plugin-based provider registry
//!
//! The registry allows DNS providers and IP sources to be registered
//! dynamically at runtime, avoiding hardcoded if-else chains.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ddns_core::registry::ProviderRegistry;
//! use ddns_core::config::ProviderConfig;
//!
//! // Create a registry
//! let mut registry = ProviderRegistry::new();
//!
//! // Register providers
//! registry.register_provider("cloudflare", Box::new(cloudflare_factory));
//!
//! // Create provider from config
//! let config = ProviderConfig::Cloudflare { ... };
//! let provider = registry.create_provider(&config)?;
//! ```
//!
//! ## Registration
//!
//! Implementations should register themselves during initialization:
//!
//! ```rust,ignore
//! # use ddns_core::registry::ProviderRegistry;
//! # use ddns_core::config::ProviderConfig;
//!
//! // In ddns-provider-cloudflare crate
//! pub fn register(registry: &mut ProviderRegistry) {
//!     registry.register_provider(
//!         "cloudflare",
//!         Box::new(CloudflareFactory),
//!     );
//! }
//! ```

use crate::config::{IpSourceConfig, ProviderConfig, ProviderConfigurable};
use crate::error::{Error, Result};
use crate::traits::{DnsProvider, IpSource, StateStore};
use crate::traits::{DnsProviderFactory, IpSourceFactory, StateStoreFactory};
use std::collections::HashMap;
use std::sync::RwLock;

/// Provider registry for plugin-based DNS provider creation
///
/// The registry maintains a map of provider type names to factory objects,
/// allowing dynamic instantiation of providers based on configuration.
///
/// ## Thread Safety
///
/// The registry uses interior mutability with RwLock, allowing concurrent
/// reads and exclusive writes.
///
/// ## Plugin Architecture
///
/// The registry supports two registration mechanisms:
///
/// 1. **DnsProviderFactory**: Legacy factory pattern (still supported)
/// 2. **ProviderConfigurable**: New plugin pattern with env var loading
///
/// The new pattern is recommended as it eliminates the need to modify
/// ddns-core when adding new providers.
#[derive(Default)]
pub struct ProviderRegistry {
    /// Registered DNS provider factories (legacy)
    providers: RwLock<HashMap<String, Box<dyn DnsProviderFactory>>>,

    /// Registered provider configurables (new plugin pattern)
    ///
    /// Each provider can register a `ProviderConfigurable` implementation
    /// that handles:
    /// - Loading config from environment variables (provider-specific prefixes)
    /// - Validating provider-specific configuration
    /// - Creating provider instances from validated config
    ///
    /// This enables zero-modification provider addition.
    configurables: RwLock<HashMap<String, Box<dyn ProviderConfigurable>>>,

    /// Registered IP source factories
    ip_sources: RwLock<HashMap<String, Box<dyn IpSourceFactory>>>,

    /// Registered state store factories
    state_stores: RwLock<HashMap<String, std::sync::Arc<dyn StateStoreFactory>>>,
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a DNS provider factory
    ///
    /// # Parameters
    ///
    /// - `name`: Provider type name (e.g., "cloudflare", "route53")
    /// - `factory`: Factory object for creating provider instances
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ddns_core::registry::ProviderRegistry;
    /// # use ddns_core::traits::DnsProviderFactory;
    /// # struct MyFactory;
    /// # impl DnsProviderFactory for MyFactory {
    /// #     fn create(&self, config: &ddns_core::config::ProviderConfig) -> ddns_core::Result<Box<dyn ddns_core::DnsProvider>> { unimplemented!() }
    /// # }
    /// let mut registry = ProviderRegistry::new();
    /// registry.register_provider("myprovider", Box::new(MyFactory));
    /// ```
    pub fn register_provider(&self, name: impl Into<String>, factory: Box<dyn DnsProviderFactory>) {
        let name = name.into();
        let mut providers = self.providers.write().unwrap();
        providers.insert(name, factory);
    }

    /// Register an IP source factory
    ///
    /// # Parameters
    ///
    /// - `name`: IP source type name (e.g., "netlink", "http")
    /// - `factory`: Factory object for creating IP source instances
    pub fn register_ip_source(&self, name: impl Into<String>, factory: Box<dyn IpSourceFactory>) {
        let name = name.into();
        let mut sources = self.ip_sources.write().unwrap();
        sources.insert(name, factory);
    }

    /// Register a state store factory
    ///
    /// # Parameters
    ///
    /// - `name`: State store type name (e.g., "file", "memory")
    /// - `factory`: Factory object for creating state store instances
    pub fn register_state_store(
        &self,
        name: impl Into<String>,
        factory: Box<dyn StateStoreFactory>,
    ) {
        let name = name.into();
        let mut stores = self.state_stores.write().unwrap();
        stores.insert(name, std::sync::Arc::from(factory));
    }

    /// Register a provider with its configuration trait
    ///
    /// This is the recommended registration method for new providers.
    /// The registry will use the provider's `ProviderConfigurable`
    /// implementation to:
    /// - Load configuration from environment variables (with provider-specific prefixes)
    /// - Validate provider-specific configuration
    /// - Create provider instances from validated config
    ///
    /// # Parameters
    ///
    /// - `configurable`: Provider's configuration handler
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ddns_core::registry::ProviderRegistry;
    /// # use ddns_core::config::ProviderConfigurable;
    /// # struct CloudflareConfigurable;
    /// # impl ProviderConfigurable for CloudflareConfigurable {
    /// #     fn name(&self) -> &'static str { "cloudflare" }
    /// #     fn load_from_env(&self) -> Result<serde_json::Value, ddns_core::Error> { unimplemented!() }
    /// #     fn validate(&self, config: &serde_json::Value) -> Result<(), ddns_core::Error> { unimplemented!() }
    /// #     fn create_provider(&self, config: &serde_json::Value, dry_run: bool) -> Result<Box<dyn ddns_core::DnsProvider>, ddns_core::Error> { unimplemented!() }
    /// # }
    /// let registry = ProviderRegistry::new();
    /// registry.register_provider_configurable(Box::new(CloudflareConfigurable));
    /// ```
    ///
    /// # Benefits
    ///
    /// - **Zero Modification**: New providers don't require ddns-core changes
    /// - **Provider-Specific Env Vars**: Each provider uses unique env var prefixes
    /// - **Self-Validating**: Providers validate their own configuration
    pub fn register_provider_configurable(&self, configurable: Box<dyn ProviderConfigurable>) {
        let name = configurable.name();
        let mut configurables = self.configurables.write().unwrap();
        configurables.insert(name.to_string(), configurable);
    }

    /// Load provider configuration from environment variables
    ///
    /// This method calls the provider's `load_from_env()` implementation,
    /// which reads provider-specific environment variables (e.g.,
    /// `CLOUDFLARE_API_TOKEN`, `ALIYUN_ACCESS_KEY_ID`, etc.).
    ///
    /// # Parameters
    ///
    /// - `provider_type`: Provider name (e.g., "cloudflare", "aliyun")
    ///
    /// # Returns
    ///
    /// Provider configuration data as JSON
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Provider type is not registered
    /// - Required environment variables are missing
    /// - Environment variables are invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ddns_core::registry::ProviderRegistry;
    /// # fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let registry = ProviderRegistry::new();
    /// let config = registry.load_provider_config("cloudflare")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_provider_config(&self, provider_type: &str) -> Result<serde_json::Value> {
        let configurables = self.configurables.read().unwrap();

        let configurable = configurables
            .get(provider_type)
            .ok_or_else(|| Error::config(format!("Unknown provider type: {}", provider_type)))?;

        configurable.load_from_env()
    }

    /// Create provider instance from configuration data
    ///
    /// This method validates the configuration and creates a provider instance.
    /// It uses the provider's `ProviderConfigurable` implementation.
    ///
    /// # Parameters
    ///
    /// - `provider_type`: Provider name (e.g., "cloudflare", "aliyun")
    /// - `config`: Configuration data (typically from `load_provider_config()`)
    /// - `dry_run`: Whether to run in dry-run mode
    ///
    /// # Returns
    ///
    /// Boxed DnsProvider trait object
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Provider type is not registered
    /// - Configuration is invalid
    /// - Provider creation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ddns_core::registry::ProviderRegistry;
    /// # fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let registry = ProviderRegistry::new();
    /// // Load config from environment
    /// let config = registry.load_provider_config("cloudflare")?;
    ///
    /// // Create provider instance
    /// let provider = registry.create_provider_from_config("cloudflare", &config, false)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_provider_from_config(
        &self,
        provider_type: &str,
        config: &serde_json::Value,
        dry_run: bool,
    ) -> Result<Box<dyn DnsProvider>> {
        let configurables = self.configurables.read().unwrap();

        let configurable = configurables
            .get(provider_type)
            .ok_or_else(|| Error::config(format!("Unknown provider type: {}", provider_type)))?;

        // Validate configuration
        configurable.validate(config)?;

        // Create provider instance
        configurable.create_provider(config, dry_run)
    }

    /// Create a DNS provider from configuration
    ///
    /// # Parameters
    ///
    /// - `config`: Provider configuration
    ///
    /// # Returns
    ///
    /// - `Ok(Box<dyn DnsProvider>)`: Created provider instance
    /// - `Err(Error)`: If provider type is not registered or creation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use ddns_core::registry::ProviderRegistry;
    /// # use ddns_core::config::ProviderConfig;
    /// # fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let registry = ProviderRegistry::new();
    /// let config = ProviderConfig {
    ///     provider_type: "cloudflare".to_string(),
    ///     config: serde_json::json!({
    ///         "api_token": "token",
    ///         "zone_id": null,
    ///     }),
    /// };
    /// let provider = registry.create_provider(&config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_provider(&self, config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
        let provider_type = config.type_name();
        let providers = self.providers.read().unwrap();

        let factory = providers
            .get(provider_type)
            .ok_or_else(|| Error::config(format!("Unknown provider type: {}", provider_type)))?;

        factory.create(config)
    }

    /// Create an IP source from configuration
    ///
    /// # Parameters
    ///
    /// - `config`: IP source configuration
    ///
    /// # Returns
    ///
    /// - `Ok(Box<dyn IpSource>)`: Created IP source instance
    /// - `Err(Error)`: If source type is not registered or creation fails
    pub fn create_ip_source(&self, config: &IpSourceConfig) -> Result<Box<dyn IpSource>> {
        let source_type = match config {
            IpSourceConfig::Netlink { .. } => "netlink",
            IpSourceConfig::Http { .. } => "http",
            IpSourceConfig::Custom { factory, .. } => factory,
        };

        let sources = self.ip_sources.read().unwrap();

        let factory = sources
            .get(source_type)
            .ok_or_else(|| Error::config(format!("Unknown IP source type: {}", source_type)))?;

        factory.create(config)
    }

    /// Create a state store from configuration
    ///
    /// # Parameters
    ///
    /// - `config`: State store configuration
    ///
    /// # Returns
    ///
    /// - `Ok(Box<dyn StateStore>)`: Created state store instance
    /// - `Err(Error)`: If store type is not registered or creation fails
    pub async fn create_state_store(
        &self,
        config: &crate::config::StateStoreConfig,
    ) -> Result<Box<dyn StateStore>> {
        let store_type = match config {
            crate::config::StateStoreConfig::File { .. } => "file",
            crate::config::StateStoreConfig::Memory => "memory",
            crate::config::StateStoreConfig::Custom { factory, .. } => factory,
        };

        // Create the state store config JSON
        let config_json = serde_json::to_value(config)?;

        // Get factory with lock held only for this scope
        let factory = {
            let stores = self.state_stores.read().unwrap();
            stores
                .get(store_type)
                .ok_or_else(|| Error::config(format!("Unknown state store type: {}", store_type)))?
                .clone()
        };

        // Lock is released before await
        factory.create(&config_json).await
    }

    /// List all registered provider types
    ///
    /// # Returns
    ///
    /// A vector of registered provider type names
    pub fn list_providers(&self) -> Vec<String> {
        let providers = self.providers.read().unwrap();
        providers.keys().cloned().collect()
    }

    /// List all registered IP source types
    ///
    /// # Returns
    ///
    /// A vector of registered IP source type names
    pub fn list_ip_sources(&self) -> Vec<String> {
        let sources = self.ip_sources.read().unwrap();
        sources.keys().cloned().collect()
    }

    /// List all registered state store types
    ///
    /// # Returns
    ///
    /// A vector of registered state store type names
    pub fn list_state_stores(&self) -> Vec<String> {
        let stores = self.state_stores.read().unwrap();
        stores.keys().cloned().collect()
    }

    /// Check if a provider type is registered
    ///
    /// # Parameters
    ///
    /// - `name`: Provider type name
    ///
    /// # Returns
    ///
    /// `true` if registered, `false` otherwise
    pub fn has_provider(&self, name: &str) -> bool {
        let providers = self.providers.read().unwrap();
        providers.contains_key(name)
    }

    /// Check if an IP source type is registered
    ///
    /// # Parameters
    ///
    /// - `name`: IP source type name
    ///
    /// # Returns
    ///
    /// `true` if registered, `false` otherwise
    pub fn has_ip_source(&self, name: &str) -> bool {
        let sources = self.ip_sources.read().unwrap();
        sources.contains_key(name)
    }

    /// Check if a state store type is registered
    ///
    /// # Parameters
    ///
    /// - `name`: State store type name
    ///
    /// # Returns
    ///
    /// `true` if registered, `false` otherwise
    pub fn has_state_store(&self, name: &str) -> bool {
        let stores = self.state_stores.read().unwrap();
        stores.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProviderFactory;

    impl DnsProviderFactory for MockProviderFactory {
        fn create(&self, _config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
            Err(Error::not_found("Mock provider not implemented"))
        }
    }

    #[test]
    fn test_registry_registration() {
        let registry = ProviderRegistry::new();

        // Initially empty
        assert!(!registry.has_provider("mock"));

        // Register
        registry.register_provider("mock", Box::new(MockProviderFactory));

        // Now present
        assert!(registry.has_provider("mock"));
        assert!(registry.list_providers().contains(&"mock".to_string()));
    }
}
