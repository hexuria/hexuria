use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::CoreResult;
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsorAllocationConfig {
    pub max_sponsored_count: u32,
    pub strategy: SponsorStrategy,
    pub default_sponsor_user_id: Option<crate::shared::ids::UserId>,
}

impl Default for SponsorAllocationConfig {
    fn default() -> Self {
        Self {
            max_sponsored_count: u32::MAX,
            strategy: SponsorStrategy::RoundRobin,
            default_sponsor_user_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SponsorStrategy {
    RoundRobin,
    PerformanceBased,
}

pub struct SponsorAllocationModule {
    #[allow(dead_code)]
    config: SponsorAllocationConfig,
}

impl SponsorAllocationModule {
    #[must_use]
    pub fn new(config: SponsorAllocationConfig) -> Self {
        Self { config }
    }
}

impl Module for SponsorAllocationModule {
    fn key(&self) -> &'static str {
        "sponsor.allocation"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[EventType::EnrollmentCreated, EventType::PackagePurchased]
    }

    fn run(&self, _ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        // v1 placeholder: the engine layer wires the actual sponsor.
        // The core module exists so the registry contract is real.
        let _ = Uuid::now_v7();
        Ok(ModuleResult::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_metadata() {
        let m = SponsorAllocationModule::new(SponsorAllocationConfig::default());
        assert_eq!(m.key(), "sponsor.allocation");
        assert_eq!(m.version(), "1.0.0");
        assert!(m.handles().contains(&EventType::EnrollmentCreated));
        assert!(m.handles().contains(&EventType::PackagePurchased));
    }
}
