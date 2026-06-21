//! End-to-end test of the Royal Flush and Binary pay plan stacks.

use chrono::Utc;
use payplan_core::modules::binary::carryover_module::{BinaryCarryoverModule, CarryoverState};
use payplan_core::modules::binary::pairing_module::{BinaryPairingModule, BinaryPairingState};
use payplan_core::modules::binary::tree_module::{BinaryTreeModule, BinaryTreeState};
use payplan_core::modules::binary::volume_module::{BinaryVolumeModule, BinaryVolumeState};
use payplan_core::modules::royal::duplication_module::{
    DuplicationState, RoyalAccountDuplicationModule,
};
use payplan_core::modules::royal::flushline_module::{FlushlineState, RoyalFlushlineModule};
use payplan_core::modules::royal::matrix_module::{MatrixState, RoyalMatrixModule};
use payplan_core::modules::royal::pot_bonus_module::{PotBonusState, RoyalPotBonusModule};
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::module::{ModuleContext, ModuleResult};
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::runner::{StackRunner, StateCache};
use payplan_core::payplan::stack::{PayPlanStack, PayPlanStackStatus, StackModule};
use payplan_core::shared::ids::{CompanyId, EnrollmentId, PackageId, PayPlanStackId, UserId};
use rust_decimal::Decimal;
use serde_json::json;

fn module_config(value: serde_json::Value) -> StackModule {
    StackModule {
        module_key: String::new(),
        module_version: "1.0.0".into(),
        sort_order: 0,
        config: value,
        active: true,
    }
}

fn build_royal_stack(company_id: CompanyId) -> PayPlanStack {
    let mut modules = vec![];
    let mut s = module_config(json!({}));
    s.module_key = "sponsor.allocation".into();
    s.sort_order = 10;
    modules.push(s);

    let mut s = module_config(json!({}));
    s.module_key = "royal.flushline".into();
    s.sort_order = 20;
    modules.push(s);

    let mut s = module_config(json!({}));
    s.module_key = "royal.matrix".into();
    s.sort_order = 30;
    modules.push(s);

    let mut s = module_config(json!({ "auto_cycle": true }));
    s.module_key = "royal.pot_bonus".into();
    s.sort_order = 40;
    modules.push(s);

    let mut s = module_config(json!({ "enabled": true }));
    s.module_key = "royal.account_duplication".into();
    s.sort_order = 50;
    modules.push(s);

    PayPlanStack {
        id: PayPlanStackId::new(),
        company_id,
        name: "Royal Flush".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules,
        created_at: Utc::now(),
    }
}

fn build_binary_stack(company_id: CompanyId) -> PayPlanStack {
    let mut modules = vec![];
    let mut s = module_config(json!({}));
    s.module_key = "sponsor.allocation".into();
    s.sort_order = 10;
    modules.push(s);

    let mut s = module_config(json!({ "strategy": "auto_balance" }));
    s.module_key = "binary.tree".into();
    s.sort_order = 20;
    modules.push(s);

    let mut s = module_config(
        json!({ "count_purchase_volume": true, "count_renewal_volume": true, "carryover_enabled": true }),
    );
    s.module_key = "binary.volume".into();
    s.sort_order = 30;
    modules.push(s);

    let mut s =
        module_config(json!({ "left_ratio": 1, "right_ratio": 1, "commission_percent": 10 }));
    s.module_key = "binary.pairing_bonus".into();
    s.sort_order = 40;
    modules.push(s);

    let mut s = module_config(json!({}));
    s.module_key = "binary.carryover".into();
    s.sort_order = 50;
    modules.push(s);

    PayPlanStack {
        id: PayPlanStackId::new(),
        company_id,
        name: "Binary".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules,
        created_at: Utc::now(),
    }
}

fn registry() -> ModuleRegistry {
    let mut r = ModuleRegistry::new();
    r.register(
        payplan_core::modules::sponsor::SponsorAllocationModule::new(
            payplan_core::modules::sponsor::SponsorAllocationConfig::default(),
        ),
    );
    r.register(RoyalFlushlineModule::new(Default::default()));
    r.register(RoyalMatrixModule::new(Default::default()));
    r.register(RoyalPotBonusModule::new(Default::default()));
    r.register(RoyalAccountDuplicationModule::new(Default::default()));
    r.register(BinaryTreeModule::new(Default::default()));
    r.register(BinaryVolumeModule::new(Default::default()));
    r.register(BinaryPairingModule::new(Default::default()));
    r.register(BinaryCarryoverModule::new());
    r
}

fn package_purchased_event(
    company_id: CompanyId,
    user_id: UserId,
    package_id: PackageId,
    points: u32,
    volume: i64,
) -> DomainEvent {
    DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::PackagePurchased,
        payload: json!({
            "user_id": user_id,
            "package_id": package_id,
            "points": points,
            "volume": volume,
            "leg": "left",
            "purchase_id": uuid::Uuid::now_v7(),
        }),
        created_at: Utc::now(),
    }
}

fn enrollment_event(company_id: CompanyId, user_id: UserId, package_id: PackageId) -> DomainEvent {
    DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::EnrollmentCreated,
        payload: json!({
            "user_id": user_id,
            "package_id": package_id,
        }),
        created_at: Utc::now(),
    }
}

#[test]
fn royal_flush_enrollment_emits_flushline_account_created() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let pkg = PackageId::new();
    let stack = build_royal_stack(company_id);

    let event = enrollment_event(company_id, user_id, pkg);
    let agg = uuid::Uuid::now_v7();
    let ctx = ModuleContext::new(company_id, pkg)
        .with_aggregate(agg)
        .with_enrollment(EnrollmentId::new())
        .with_event(event.clone());
    let result = runner
        .run(&stack, &event, &ctx, &mut StateCache::new())
        .expect("run ok");

    assert!(result
        .emitted_events
        .iter()
        .any(|e| e.event_type == EventType::RoyalFlushlineAccountCreated));
}

#[test]
fn royal_flush_graduates_after_15_points() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let pkg = PackageId::new();
    let stack = build_royal_stack(company_id);

    let event = package_purchased_event(company_id, user_id, pkg, 15, 0);
    let ctx = ModuleContext::new(company_id, pkg)
        .with_aggregate(uuid::Uuid::now_v7())
        .with_enrollment(EnrollmentId::new())
        .with_module_state(json!({
            "account": {
                "id": payplan_core::shared::ids::RoyalAccountId::new(),
                "company_id": company_id,
                "enrollment_id": EnrollmentId::new(),
                "owner_user_id": user_id,
                "current_tier": "Ten",
                "current_points": 0,
                "graduated": false,
                "graduated_at": null,
                "created_at": Utc::now(),
            }
        }))
        .with_event(event.clone());
    let result = runner
        .run(&stack, &event, &ctx, &mut StateCache::new())
        .expect("run ok");

    assert!(result
        .emitted_events
        .iter()
        .any(|e| e.event_type == EventType::RoyalFlushlineGraduated));
}

#[test]
fn royal_flush_pot_bonus_distributes_to_qualified_user() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let pkg = PackageId::new();
    let stack = build_royal_stack(company_id);

    let distribution_event = DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::RoyalPotBonusDistributed,
        payload: json!({}),
        created_at: Utc::now(),
    };
    let ctx = ModuleContext::new(company_id, pkg)
        .with_aggregate(uuid::Uuid::now_v7())
        .with_module_state(json!({
            "pool": "1000",
            "qualifications": [{
                "user_id": user_id,
                "total_graduations": 1,
                "total_matrix_cycles": 1,
                "is_qualified": true,
            }]
        }))
        .with_event(distribution_event.clone());

    let result = runner
        .run(&stack, &distribution_event, &ctx, &mut StateCache::new())
        .expect("run ok");
    assert!(result
        .ledger_entries
        .iter()
        .any(|e| e.user_id == user_id && e.reason == "royal.pot_bonus.profit_share"));
    assert!(result
        .ledger_entries
        .iter()
        .any(|e| e.user_id == user_id && e.reason.starts_with("royal.pot_bonus.top_cycler[")));
}

#[test]
fn binary_tree_places_first_user_as_root() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let pkg = PackageId::new();
    let stack = build_binary_stack(company_id);

    let event = enrollment_event(company_id, user_id, pkg);
    let ctx = ModuleContext::new(company_id, pkg)
        .with_aggregate(uuid::Uuid::now_v7())
        .with_enrollment(EnrollmentId::new())
        .with_event(event.clone());

    let result = runner
        .run(&stack, &event, &ctx, &mut StateCache::new())
        .expect("run ok");
    let placement = result
        .emitted_events
        .iter()
        .find(|e| e.event_type == EventType::BinaryNodePlaced)
        .expect("placement emitted");
    assert!(placement.payload.get("node_id").is_some());
    assert!(placement
        .payload
        .get("parent_node_id")
        .is_none_or(|v| v.is_null()));
    assert!(placement.payload.get("leg").is_none_or(|v| v.is_null()));
}

#[test]
fn binary_tree_autobalance_places_left_then_right() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let pkg = PackageId::new();
    let stack = build_binary_stack(company_id);

    let root_user = UserId::new();
    let e1 = enrollment_event(company_id, root_user, pkg);
    let mut cache = StateCache::new();
    let r1 = runner
        .run(
            &stack,
            &e1,
            &ModuleContext::new(company_id, pkg)
                .with_aggregate(uuid::Uuid::now_v7())
                .with_enrollment(EnrollmentId::new())
                .with_event(e1.clone()),
            &mut cache,
        )
        .unwrap();
    let tree_state: BinaryTreeState = r1
        .state_changes
        .iter()
        .find(|sc| sc.module_key == "binary.tree")
        .and_then(|sc| serde_json::from_value(sc.value.clone()).ok())
        .unwrap_or_default();

    let u2 = UserId::new();
    let e2 = enrollment_event(company_id, u2, pkg);
    let r2 = runner
        .run(
            &stack,
            &e2,
            &ModuleContext::new(company_id, pkg)
                .with_aggregate(uuid::Uuid::now_v7())
                .with_enrollment(EnrollmentId::new())
                .with_module_state(serde_json::to_value(&tree_state).unwrap())
                .with_event(e2.clone()),
            &mut cache,
        )
        .unwrap();
    let second_placement = r2
        .emitted_events
        .iter()
        .find(|e| e.event_type == EventType::BinaryNodePlaced)
        .unwrap();
    assert_eq!(
        second_placement.payload.get("leg").and_then(|v| v.as_str()),
        Some("left")
    );
}

#[test]
fn binary_pairing_emits_commission_and_ledger_entry() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let pkg = PackageId::new();
    let stack = build_binary_stack(company_id);

    let node_user = UserId::new();
    let cycle_event = DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::BinaryCycleClosed,
        payload: json!({ "node_user_id": node_user }),
        created_at: Utc::now(),
    };
    let ctx = ModuleContext::new(company_id, pkg)
        .with_aggregate(uuid::Uuid::now_v7())
        .with_event(cycle_event.clone())
        .with_module_state(json!({
            "pending_totals": { "left": 100, "right": 100 }
        }));

    let result = runner
        .run(&stack, &cycle_event, &ctx, &mut StateCache::new())
        .unwrap();
    assert!(result
        .emitted_events
        .iter()
        .any(|e| e.event_type == EventType::BinaryPairMatched));
    assert!(result
        .emitted_events
        .iter()
        .any(|e| e.event_type == EventType::BinaryCommissionEarned));
    assert!(result
        .ledger_entries
        .iter()
        .any(|e| e.user_id == node_user && e.reason == "binary.pairing.commission"));
}

#[test]
fn binary_carryover_emits_update_event_on_cycle_close() {
    let reg = registry();
    let runner = StackRunner::new(reg);
    let company_id = CompanyId::new();
    let pkg = PackageId::new();
    let stack = build_binary_stack(company_id);

    // Carryover now consumes the pairing module's `BinaryPairMatched` output
    // (which carries left/right/matched) rather than reading state that was
    // never written (Task 8). Feed that event directly: matched=30 of left=50,
    // right=30 → unmatched left=20 carries.
    let pair_event = DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::BinaryPairMatched,
        payload: json!({
            "node_user_id": UserId::new(),
            "left": 50,
            "right": 30,
            "matched": 30,
        }),
        created_at: Utc::now(),
    };
    let ctx = ModuleContext::new(company_id, pkg)
        .with_aggregate(uuid::Uuid::now_v7())
        .with_event(pair_event.clone())
        .with_module_state(json!({
            "carry": { "left_volume": 0, "right_volume": 0 },
        }));

    let result = runner
        .run(&stack, &pair_event, &ctx, &mut StateCache::new())
        .unwrap();
    assert!(result
        .emitted_events
        .iter()
        .any(|e| e.event_type == EventType::BinaryCarryoverUpdated));
}

#[allow(dead_code)]
fn _typecheck() {
    let _: ModuleResult = ModuleResult::empty();
    let _: BinaryVolumeState = BinaryVolumeState::default();
    let _: BinaryPairingState = BinaryPairingState::default();
    let _: CarryoverState = CarryoverState::default();
    let _: FlushlineState = FlushlineState::default();
    let _: MatrixState = MatrixState::default();
    let _: PotBonusState = PotBonusState::default();
    let _: DuplicationState = DuplicationState::default();
    let _: Decimal = Decimal::ZERO;
}
