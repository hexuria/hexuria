use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::payplan::events::{DomainEvent, EventType};
use crate::payplan::ledger::RewardLedgerEntry;
use crate::shared::ids::{EnrollmentId, EventId, PackageId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleResult {
    /// Events this module emitted as a side effect of `run`.
    pub emitted_events: Vec<DomainEvent>,
    /// Reward ledger entries this module created.
    pub ledger_entries: Vec<RewardLedgerEntry>,
    /// New module-specific state to persist. The engine layer writes this back to the
    /// state store keyed by `(module_key, module_version, aggregate_id)`.
    pub state_change: Option<Value>,
    /// Human-readable warnings (do NOT block execution).
    pub warnings: Vec<String>,
}

impl ModuleResult {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            emitted_events: vec![],
            ledger_entries: vec![],
            state_change: None,
            warnings: vec![],
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.emitted_events.is_empty()
            && self.ledger_entries.is_empty()
            && self.state_change.is_none()
            && self.warnings.is_empty()
    }

    pub fn emit(&mut self, event_type: EventType, payload: Value) {
        self.emitted_events.push(DomainEvent {
            id: EventId::new(),
            event_type,
            payload,
            created_at: Utc::now(),
        });
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    pub fn set_state(&mut self, value: Value) {
        self.state_change = Some(value);
    }
}

/// Context handed to a module on every invocation. The engine loads it from
/// persisted state before calling `run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleContext {
    pub package_id: PackageId,
    pub enrollment_id: Option<EnrollmentId>,
    /// The aggregate ID used for state persistence (the `module_state` table's
    /// primary key includes this). Defaults to `enrollment_id.0` when the
    /// enrollment is known, otherwise the engine must set it explicitly via
    /// `with_aggregate`.
    pub aggregate_id: Option<uuid::Uuid>,
    /// The event that triggered this module invocation.
    pub triggering_event: Option<DomainEvent>,
    /// Opaque blob holding the module's previous persisted state. Modules decode this
    /// into their own typed state struct; an empty object `{}` means "no prior state".
    pub module_state: Value,
    /// Module-specific config from the stack entry.
    pub module_config: Value,
    /// Convenience timestamp the module can use instead of calling `Utc::now()`.
    pub now: DateTime<Utc>,
}

impl ModuleContext {
    #[must_use]
    pub fn new(package_id: PackageId) -> Self {
        Self {
            package_id,
            enrollment_id: None,
            aggregate_id: None,
            triggering_event: None,
            module_state: Value::Null,
            module_config: Value::Object(Default::default()),
            now: Utc::now(),
        }
    }

    #[must_use]
    pub fn with_enrollment(mut self, id: EnrollmentId) -> Self {
        self.enrollment_id = Some(id);
        self.aggregate_id = Some(id.0);
        self
    }

    #[must_use]
    pub fn with_aggregate(mut self, aggregate_id: uuid::Uuid) -> Self {
        self.aggregate_id = Some(aggregate_id);
        self
    }

    #[must_use]
    pub fn with_event(mut self, event: DomainEvent) -> Self {
        self.triggering_event = Some(event);
        self
    }

    #[must_use]
    pub fn with_module_state(mut self, state: Value) -> Self {
        self.module_state = state;
        self
    }

    #[must_use]
    pub fn with_module_config(mut self, config: Value) -> Self {
        self.module_config = config;
        self
    }

    /// Resolve the aggregate ID for state persistence.
    ///
    /// Modules read this to know which aggregate their state should be scoped
    /// to. Returns `None` if no aggregate has been set — the caller is then
    /// responsible for either failing or substituting a fallback.
    #[must_use]
    pub fn state_aggregate(&self) -> Option<uuid::Uuid> {
        self.aggregate_id
            .or_else(|| self.enrollment_id.map(|e| e.0))
    }

    /// Helper to decode module_state into a typed struct, defaulting to `T::default()` when missing.
    pub fn decode_state<T: serde::de::DeserializeOwned + Default>(
        &self,
    ) -> Result<T, serde_json::Error> {
        if self.module_state.is_null() {
            Ok(T::default())
        } else {
            serde_json::from_value(self.module_state.clone())
        }
    }
}
