use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::{CoreError, CoreResult};
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};

/// A pay plan compensation module.
///
/// Modules are pure: they receive a context plus their own config and current state,
/// and return new state, emitted events, ledger entries, and any warnings.
///
/// Modules MUST NOT:
/// - call payment gateways, email, or external services
/// - mutate another module's state
/// - write directly to a database
pub trait Module: Send + Sync {
    /// Stable key used to look the module up in the registry (e.g. `"royal.flushline"`).
    fn key(&self) -> &'static str;

    /// Semver-ish version string for the module implementation (e.g. `"1.0.0"`).
    fn version(&self) -> &'static str;

    /// Events this module is interested in. The engine will skip modules whose
    /// `handles()` list does not contain the triggering event.
    fn handles(&self) -> &'static [EventType];

    /// Run the module for a given triggering event.
    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult>;
}

/// Keyed registry of module implementations keyed by `(key, version)`.
#[derive(Default, Clone)]
pub struct ModuleRegistry {
    modules: BTreeMap<(String, String), Arc<dyn Module>>,
}

impl ModuleRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<M: Module + 'static>(&mut self, module: M) -> &mut Self {
        self.modules.insert(
            (module.key().to_string(), module.version().to_string()),
            Arc::new(module),
        );
        self
    }

    #[must_use]
    pub fn get(&self, key: &str, version: &str) -> Option<Arc<dyn Module>> {
        self.modules
            .get(&(key.to_string(), version.to_string()))
            .cloned()
    }

    #[must_use]
    pub fn keys(&self) -> Vec<(String, String)> {
        self.modules.keys().cloned().collect()
    }

    pub fn require(&self, key: &str, version: &str) -> CoreResult<Arc<dyn Module>> {
        self.get(key, version)
            .ok_or_else(|| CoreError::Validation(format!("module {key}@{version} not registered")))
    }
}

impl std::fmt::Debug for ModuleRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleRegistry")
            .field("modules", &self.keys())
            .finish()
    }
}
