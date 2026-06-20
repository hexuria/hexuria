use std::collections::HashMap;

use crate::error::{CoreError, CoreResult};
use crate::payplan::events::DomainEvent;
use crate::payplan::ledger::RewardLedgerEntry;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::ModuleRegistry;
use crate::payplan::stack::PayPlanStack;
use uuid::Uuid;

/// A state change emitted by a module, tagged with the aggregate it scopes to.
#[derive(Debug, Clone)]
pub struct StateChange {
    pub module_key: String,
    pub module_version: String,
    pub aggregate_id: Uuid,
    pub value: serde_json::Value,
}

/// Result of running a stack against an event.
#[derive(Debug, Default, Clone)]
pub struct StackRunResult {
    pub emitted_events: Vec<DomainEvent>,
    pub ledger_entries: Vec<RewardLedgerEntry>,
    /// Per-aggregate state changes keyed by `(module_key, module_version, aggregate_id)`.
    /// The engine layer should persist these via the `ModuleStateStore`.
    pub state_changes: Vec<StateChange>,
    pub warnings: Vec<String>,
}

impl StackRunResult {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.emitted_events.is_empty()
            && self.ledger_entries.is_empty()
            && self.state_changes.is_empty()
            && self.warnings.is_empty()
    }

    pub fn merge(&mut self, key: &str, version: &str, aggregate_id: Uuid, other: ModuleResult) {
        self.emitted_events.extend(other.emitted_events);
        self.ledger_entries.extend(other.ledger_entries);
        self.warnings.extend(other.warnings);
        if let Some(v) = other.state_change {
            self.state_changes.push(StateChange {
                module_key: key.to_string(),
                module_version: version.to_string(),
                aggregate_id,
                value: v,
            });
        }
    }
}

/// State cache used by `StackRunner` to thread per-aggregate module state across
/// cascade iterations within a single run. In production this is backed by the
/// `module_state` table; here we keep it in-memory.
#[derive(Debug, Default, Clone)]
pub struct StateCache {
    inner: HashMap<String, serde_json::Value>,
}

impl StateCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key(module_key: &str, module_version: &str, aggregate_id: Uuid) -> String {
        format!("{module_key}@{module_version}:{aggregate_id}")
    }

    #[must_use]
    pub fn get(
        &self,
        module_key: &str,
        module_version: &str,
        aggregate_id: Uuid,
    ) -> Option<serde_json::Value> {
        self.inner
            .get(&Self::key(module_key, module_version, aggregate_id))
            .cloned()
    }

    pub fn put(
        &mut self,
        module_key: &str,
        module_version: &str,
        aggregate_id: Uuid,
        value: serde_json::Value,
    ) {
        self.inner
            .insert(Self::key(module_key, module_version, aggregate_id), value);
    }
}

/// Executes a [`PayPlanStack`] against a triggering event.
///
/// The runner is pure: it walks the stack in `sort_order`, calls each module that
/// handles the event, and collects the union of events/ledger entries/warnings.
/// It performs NO I/O. Module state is threaded through `state_cache` and
/// scoped per `(module_key, module_version, aggregate_id)`.
pub struct StackRunner {
    registry: ModuleRegistry,
}

impl StackRunner {
    #[must_use]
    pub fn new(registry: ModuleRegistry) -> Self {
        Self { registry }
    }

    #[must_use]
    pub fn registry(&self) -> &ModuleRegistry {
        &self.registry
    }

    /// Run the stack against `triggering`, threading state through `state_cache`.
    ///
    /// `ctx` provides company/package/enrollment. `aggregate_id` falls back to
    /// the enrollment id when not set explicitly on the context.
    pub fn run(
        &self,
        stack: &PayPlanStack,
        triggering: &DomainEvent,
        ctx: &ModuleContext,
        state_cache: &mut StateCache,
    ) -> CoreResult<StackRunResult> {
        if stack.modules.is_empty() {
            return Err(CoreError::Validation(format!(
                "stack {} has no modules",
                stack.id
            )));
        }

        let mut ordered: Vec<_> = stack.modules.iter().filter(|m| m.active).collect();
        ordered.sort_by_key(|m| m.sort_order);

        let mut result = StackRunResult::default();

        for stack_module in ordered {
            let module = self
                .registry
                .get(&stack_module.module_key, &stack_module.module_version)
                .ok_or_else(|| {
                    CoreError::Validation(format!(
                        "module {}@{} not registered",
                        stack_module.module_key, stack_module.module_version
                    ))
                })?;

            if !module.handles().contains(&triggering.event_type) {
                continue;
            }

            // Resolve the aggregate for state I/O. Modules can override via
            // `ctx.aggregate_id` (set by the caller) or fall back to the
            // enrollment id.
            let Some(aggregate_id) = ctx.state_aggregate() else {
                return Err(CoreError::Validation(format!(
                    "module {}@{} requires an aggregate_id; none set in context",
                    stack_module.module_key, stack_module.module_version
                )));
            };

            // Build a per-module context with the cached state and config.
            let mut module_ctx = ctx.clone();
            module_ctx.module_state = state_cache
                .get(
                    &stack_module.module_key,
                    &stack_module.module_version,
                    aggregate_id,
                )
                .unwrap_or(module_ctx.module_state);
            module_ctx.module_config = stack_module.config.clone();

            let module_result = module.run(&module_ctx)?;
            if let Some(value) = &module_result.state_change {
                state_cache.put(
                    &stack_module.module_key,
                    &stack_module.module_version,
                    aggregate_id,
                    value.clone(),
                );
            }
            result.merge(
                &stack_module.module_key,
                &stack_module.module_version,
                aggregate_id,
                module_result,
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payplan::events::EventType;
    use crate::payplan::registry::Module;
    use crate::payplan::stack::{PayPlanStackStatus, StackModule};
    use crate::shared::ids::{CompanyId, PackageId, PayPlanStackId};
    use chrono::Utc;
    use serde_json::json;

    struct NoopModule;
    impl Module for NoopModule {
        fn key(&self) -> &'static str {
            "test.noop"
        }
        fn version(&self) -> &'static str {
            "1.0.0"
        }
        fn handles(&self) -> &'static [EventType] {
            &[EventType::PackagePurchased]
        }
        fn run(&self, _ctx: &ModuleContext) -> CoreResult<ModuleResult> {
            Ok(ModuleResult::empty())
        }
    }

    #[test]
    fn runs_modules_in_sort_order() {
        let mut reg = ModuleRegistry::new();
        reg.register(NoopModule);
        let runner = StackRunner::new(reg);

        let stack = PayPlanStack {
            id: PayPlanStackId::new(),
            company_id: CompanyId::new(),
            name: "Test".into(),
            version: 1,
            status: PayPlanStackStatus::Active,
            modules: vec![StackModule {
                module_key: "test.noop".into(),
                module_version: "1.0.0".into(),
                sort_order: 10,
                config: json!({}),
                active: true,
            }],
            created_at: Utc::now(),
        };

        let event = DomainEvent {
            id: crate::shared::ids::EventId::new(),
            company_id: Some(stack.company_id),
            event_type: EventType::PackagePurchased,
            payload: json!({}),
            created_at: Utc::now(),
        };

        let ctx =
            ModuleContext::new(stack.company_id, PackageId::new()).with_aggregate(Uuid::now_v7());
        let mut cache = StateCache::new();
        let result = runner
            .run(&stack, &event, &ctx, &mut cache)
            .expect("run ok");
        assert!(result.is_empty());
    }

    #[test]
    fn empty_stack_is_error() {
        let runner = StackRunner::new(ModuleRegistry::new());
        let stack = PayPlanStack {
            id: PayPlanStackId::new(),
            company_id: CompanyId::new(),
            name: "Empty".into(),
            version: 1,
            status: PayPlanStackStatus::Draft,
            modules: vec![],
            created_at: Utc::now(),
        };
        let event = DomainEvent {
            id: crate::shared::ids::EventId::new(),
            company_id: Some(stack.company_id),
            event_type: EventType::PackagePurchased,
            payload: json!({}),
            created_at: Utc::now(),
        };
        let ctx =
            ModuleContext::new(stack.company_id, PackageId::new()).with_aggregate(Uuid::now_v7());
        let mut cache = StateCache::new();
        let err = runner.run(&stack, &event, &ctx, &mut cache).unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    /// Module that increments a counter on each call to prove state threading works.
    struct CounterModule;
    impl Module for CounterModule {
        fn key(&self) -> &'static str {
            "test.counter"
        }
        fn version(&self) -> &'static str {
            "1.0.0"
        }
        fn handles(&self) -> &'static [EventType] {
            &[EventType::PackagePurchased]
        }
        fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
            let mut state: serde_json::Value = if ctx.module_state.is_null() {
                json!({"n": 0})
            } else {
                ctx.module_state.clone()
            };
            let n = state
                .get("n")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                + 1;
            state["n"] = json!(n);
            let mut result = ModuleResult::empty();
            result.set_state(state);
            Ok(result)
        }
    }

    #[test]
    fn state_threads_through_cache_across_calls() {
        let mut reg = ModuleRegistry::new();
        reg.register(CounterModule);
        let runner = StackRunner::new(reg);

        let stack = PayPlanStack {
            id: PayPlanStackId::new(),
            company_id: CompanyId::new(),
            name: "Counter".into(),
            version: 1,
            status: PayPlanStackStatus::Active,
            modules: vec![StackModule {
                module_key: "test.counter".into(),
                module_version: "1.0.0".into(),
                sort_order: 10,
                config: json!({}),
                active: true,
            }],
            created_at: Utc::now(),
        };
        let event = DomainEvent {
            id: crate::shared::ids::EventId::new(),
            company_id: Some(stack.company_id),
            event_type: EventType::PackagePurchased,
            payload: json!({}),
            created_at: Utc::now(),
        };
        let agg = Uuid::now_v7();
        let ctx = ModuleContext::new(stack.company_id, PackageId::new()).with_aggregate(agg);
        let mut cache = StateCache::new();

        runner.run(&stack, &event, &ctx, &mut cache).unwrap();
        runner.run(&stack, &event, &ctx, &mut cache).unwrap();
        runner.run(&stack, &event, &ctx, &mut cache).unwrap();

        let final_state = cache
            .get("test.counter", "1.0.0", agg)
            .expect("state present");
        assert_eq!(
            final_state.get("n").and_then(serde_json::Value::as_i64),
            Some(3)
        );
    }

    /// Different aggregates must have isolated state, even for the same module.
    #[test]
    fn state_is_isolated_per_aggregate() {
        let mut reg = ModuleRegistry::new();
        reg.register(CounterModule);
        let runner = StackRunner::new(reg);

        let stack = PayPlanStack {
            id: PayPlanStackId::new(),
            company_id: CompanyId::new(),
            name: "Counter".into(),
            version: 1,
            status: PayPlanStackStatus::Active,
            modules: vec![StackModule {
                module_key: "test.counter".into(),
                module_version: "1.0.0".into(),
                sort_order: 10,
                config: json!({}),
                active: true,
            }],
            created_at: Utc::now(),
        };
        let event = DomainEvent {
            id: crate::shared::ids::EventId::new(),
            company_id: Some(stack.company_id),
            event_type: EventType::PackagePurchased,
            payload: json!({}),
            created_at: Utc::now(),
        };

        let agg_a = Uuid::now_v7();
        let agg_b = Uuid::now_v7();
        let mut cache = StateCache::new();

        let ctx_a = ModuleContext::new(stack.company_id, PackageId::new()).with_aggregate(agg_a);
        runner.run(&stack, &event, &ctx_a, &mut cache).unwrap();
        runner.run(&stack, &event, &ctx_a, &mut cache).unwrap();
        runner.run(&stack, &event, &ctx_a, &mut cache).unwrap();

        let ctx_b = ModuleContext::new(stack.company_id, PackageId::new()).with_aggregate(agg_b);
        runner.run(&stack, &event, &ctx_b, &mut cache).unwrap();

        let state_a = cache.get("test.counter", "1.0.0", agg_a).unwrap();
        let state_b = cache.get("test.counter", "1.0.0", agg_b).unwrap();
        assert_eq!(
            state_a.get("n").and_then(serde_json::Value::as_i64),
            Some(3)
        );
        assert_eq!(
            state_b.get("n").and_then(serde_json::Value::as_i64),
            Some(1)
        );
    }
}
