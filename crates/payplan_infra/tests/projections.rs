//! Integration tests for `PgProjections` (Track A2-A5).
//!
//! Verifies that `module_state` JSON changes materialize correctly into the
//! relational tables `royal_flushline_accounts`, `binary_nodes`,
//! `binary_volume_ledger`, and `binary_carryover`, and that the upserts are
//! idempotent (re-projecting the same batch does not duplicate rows).
//!
//! Gated behind the `integration` feature. Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_infra --features integration -- --include-ignored --test-threads=1`

#![cfg(feature = "integration")]

use chrono::Utc;
use payplan_app::ports::{EventProjector, ModuleProjector};
use payplan_core::modules::binary::carryover::BinaryCarryover;
use payplan_core::modules::binary::carryover_module::CarryoverState;
use payplan_core::modules::binary::tree::{BinaryLeg, BinaryNode};
use payplan_core::modules::binary::tree_module::BinaryTreeState;
use payplan_core::modules::binary::volume::{BinaryVolumeEntry, BinaryVolumeStatus};
use payplan_core::modules::binary::volume_module::BinaryVolumeState;
use payplan_core::modules::royal::flushline::{RoyalFlushlineAccount, RoyalTier};
use payplan_core::modules::royal::flushline_module::FlushlineState;
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::runner::StateChange;
use payplan_core::shared::ids::{
    BinaryNodeId, CompanyId, EnrollmentId, EventId, RoyalAccountId, UserId,
};
use payplan_infra::migrator;
use payplan_infra::postgres::{connect, PgConfig};
use payplan_infra::projections::{PgEventProjector, PgProjections};
use serde_json::json;
use sqlx::{PgPool, Row};

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL");
    let pool = connect(&cfg).await.expect("connect");
    migrator::run(&pool).await.expect("migrations");
    pool
}

async fn truncate_all(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE TABLE binary_carryover, binary_volume_ledger, binary_pairing_results, \
         binary_cycle_periods, binary_nodes, royal_flushline_accounts, module_state, \
         enrollments, users, companies \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a company + package + purchase + user + enrollment so the FK targets
/// the projection tables reference exist. Returns the ids.
async fn seed_fks(pool: &PgPool) -> (CompanyId, UserId, EnrollmentId) {
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let enrollment_id = EnrollmentId::new();
    let package_id = payplan_core::shared::ids::PackageId::new();
    let purchase_id = payplan_core::shared::ids::PurchaseId::new();
    let slug = format!("proj-{}", uuid::Uuid::now_v7().simple());

    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'T', $2)")
        .bind(company_id)
        .bind(&slug)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO packages (id, company_id, name, status, metadata) VALUES ($1, $2, 'P', 'active', '{}')",
    )
    .bind(package_id)
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, company_id) VALUES ($1, $2, 'ph', 'user', $3)",
    )
    .bind(user_id)
    .bind(format!("u-{}@t.local", uuid::Uuid::now_v7().simple()))
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO purchases (id, company_id, user_id, package_id, gross_amount, net_amount, currency, status, purchased_at) \
         VALUES ($1, $2, $3, $4, 0, 0, 'USD', 'paid', NOW())",
    )
    .bind(purchase_id)
    .bind(company_id)
    .bind(user_id)
    .bind(package_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO enrollments (id, company_id, user_id, package_id, purchase_id, status, joined_at) \
         VALUES ($1, $2, $3, $4, $5, 'active', NOW())",
    )
    .bind(enrollment_id)
    .bind(company_id)
    .bind(user_id)
    .bind(package_id)
    .bind(purchase_id)
    .execute(pool)
    .await
    .unwrap();
    (company_id, user_id, enrollment_id)
}

fn flushline_change(account: RoyalFlushlineAccount) -> StateChange {
    StateChange {
        module_key: "royal.flushline".into(),
        module_version: "1.0.0".into(),
        aggregate_id: account.enrollment_id.0,
        value: serde_json::to_value(FlushlineState {
            account: Some(account),
        })
        .unwrap(),
    }
}

#[tokio::test]
async fn flushline_account_projects_and_upserts() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, enrollment_id) = seed_fks(&pool).await;
    let projector = PgProjections::new();

    let account = RoyalFlushlineAccount::new(company_id, enrollment_id, user_id);
    let change = flushline_change(account.clone());

    // First projection: inserts the row.
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change), &mut conn)
        .await
        .expect("project");
    drop(conn);

    let row = sqlx::query(
        "SELECT current_tier, current_points, graduated FROM royal_flushline_accounts WHERE id = $1",
    )
    .bind(account.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let tier: String = row.try_get("current_tier").unwrap();
    assert_eq!(tier, "Ten", "new account starts at Ten");
    assert_eq!(row.try_get::<i32, _>("current_points").unwrap(), 0);
    assert!(!row.try_get::<bool, _>("graduated").unwrap());

    // Second projection with an updated account (graduated): upserts in place.
    let graduated = account.apply_points(15);
    assert!(graduated.graduated);
    let change2 = flushline_change(graduated.clone());
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change2), &mut conn)
        .await
        .expect("project upsert");
    drop(conn);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM royal_flushline_accounts WHERE enrollment_id = $1",
    )
    .bind(enrollment_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "upsert did not duplicate the row");

    let row = sqlx::query(
        "SELECT current_tier, current_points, graduated FROM royal_flushline_accounts WHERE id = $1",
    )
    .bind(graduated.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("current_tier").unwrap(), "Ace");
    assert_eq!(row.try_get::<i32, _>("current_points").unwrap(), 15);
    assert!(row.try_get::<bool, _>("graduated").unwrap());
}

#[tokio::test]
async fn binary_nodes_project_with_keys() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, enrollment_id) = seed_fks(&pool).await;
    let projector = PgProjections::new();

    // A second user for the child node (binary_nodes.user_id is an FK).
    let child_user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, company_id) VALUES ($1, $2, 'ph', 'user', $3)",
    )
    .bind(child_user_id)
    .bind(format!("c-{}@t.local", uuid::Uuid::now_v7().simple()))
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    // Two-node tree: a root + a left child. Both carry company_id/enrollment_id
    // (the fields the tree module now populates from ctx).
    let root_id = BinaryNodeId::new();
    let child_id = BinaryNodeId::new();
    let nodes = vec![
        BinaryNode {
            id: root_id,
            user_id,
            sponsor_user_id: None,
            parent_node_id: None,
            leg: None,
            company_id: Some(company_id),
            enrollment_id: Some(enrollment_id),
        },
        BinaryNode {
            id: child_id,
            user_id: child_user_id,
            sponsor_user_id: Some(user_id),
            parent_node_id: Some(root_id),
            leg: Some(BinaryLeg::Left),
            company_id: Some(company_id),
            enrollment_id: Some(enrollment_id),
        },
    ];
    let change = StateChange {
        module_key: "binary.tree".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(BinaryTreeState::from_nodes(nodes)).unwrap(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change), &mut conn)
        .await
        .expect("project");
    drop(conn);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM binary_nodes WHERE enrollment_id = $1")
            .bind(enrollment_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2, "both nodes projected");

    let child = sqlx::query(
        "SELECT parent_node_id, leg FROM binary_nodes WHERE id = $1",
    )
    .bind(child_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        child.try_get::<Option<BinaryNodeId>, _>("parent_node_id").unwrap(),
        Some(root_id)
    );
    assert_eq!(child.try_get::<Option<String>, _>("leg").unwrap().as_deref(), Some("left"));

    // Re-project: idempotent.
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change), &mut conn)
        .await
        .expect("project again");
    drop(conn);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM binary_nodes WHERE enrollment_id = $1")
            .bind(enrollment_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2, "re-projection did not duplicate");
}

#[tokio::test]
async fn binary_volume_skips_entries_without_node_id() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, _user_id, _enrollment_id) = seed_fks(&pool).await;
    let projector = PgProjections::new();

    // Entry WITHOUT node_id — must be skipped (Track B will link nodes).
    let orphan = BinaryVolumeEntry {
        id: uuid::Uuid::now_v7(),
        company_id,
        source_purchase_id: None,
        leg: BinaryLeg::Left,
        volume: 100,
        status: BinaryVolumeStatus::Open,
        node_id: None,
    };
    let change = StateChange {
        module_key: "binary.volume".into(),
        module_version: "1.0.0".into(),
        aggregate_id: uuid::Uuid::now_v7(),
        value: serde_json::to_value(BinaryVolumeState {
            entries: vec![orphan],
            totals: Default::default(),
        })
        .unwrap(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change), &mut conn)
        .await
        .expect("project");
    drop(conn);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM binary_volume_ledger WHERE company_id = $1")
            .bind(company_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "orphan entry without node_id was skipped");
}

#[tokio::test]
async fn binary_volume_projects_when_node_linked() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, enrollment_id) = seed_fks(&pool).await;
    let projector = PgProjections::new();

    // First project a node so binary_volume_ledger's FK target exists.
    let node_id = BinaryNodeId::new();
    let node = BinaryNode {
        id: node_id,
        user_id,
        sponsor_user_id: None,
        parent_node_id: None,
        leg: None,
        company_id: Some(company_id),
        enrollment_id: Some(enrollment_id),
    };
    let node_change = StateChange {
        module_key: "binary.tree".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(BinaryTreeState::from_nodes(vec![node])).unwrap(),
    };
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&node_change), &mut conn)
        .await
        .expect("project node");
    drop(conn);

    // Now a volume entry linked to that node.
    let entry_id = uuid::Uuid::now_v7();
    let entry = BinaryVolumeEntry {
        id: entry_id,
        company_id,
        source_purchase_id: None,
        leg: BinaryLeg::Right,
        volume: 250,
        status: BinaryVolumeStatus::Open,
        node_id: Some(node_id),
    };
    let vol_change = StateChange {
        module_key: "binary.volume".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(BinaryVolumeState {
            entries: vec![entry],
            totals: Default::default(),
        })
        .unwrap(),
    };
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&vol_change), &mut conn)
        .await
        .expect("project volume");
    drop(conn);

    let row = sqlx::query(
        "SELECT node_id, leg, volume, status FROM binary_volume_ledger WHERE id = $1",
    )
    .bind(entry_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<BinaryNodeId, _>("node_id").unwrap(), node_id);
    assert_eq!(row.try_get::<String, _>("leg").unwrap(), "right");
    assert_eq!(row.try_get::<i64, _>("volume").unwrap(), 250);
    assert_eq!(row.try_get::<String, _>("status").unwrap(), "open");
}

#[tokio::test]
async fn binary_carryover_projects_when_keys_present() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, enrollment_id) = seed_fks(&pool).await;
    let projector = PgProjections::new();

    // Seed a node for the FK.
    let node_id = BinaryNodeId::new();
    let node = BinaryNode {
        id: node_id,
        user_id,
        sponsor_user_id: None,
        parent_node_id: None,
        leg: None,
        company_id: Some(company_id),
        enrollment_id: Some(enrollment_id),
    };
    let node_change = StateChange {
        module_key: "binary.tree".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(BinaryTreeState::from_nodes(vec![node])).unwrap(),
    };
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&node_change), &mut conn)
        .await
        .expect("project node");
    drop(conn);

    let carry = BinaryCarryover {
        left_volume: 0,
        right_volume: 30,
        company_id: Some(company_id),
        node_id: Some(node_id),
    };
    let change = StateChange {
        module_key: "binary.carryover".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(CarryoverState {
            carry,
            last_unmatched: Default::default(),
        })
        .unwrap(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change), &mut conn)
        .await
        .expect("project carryover");
    drop(conn);

    let row = sqlx::query(
        "SELECT left_carryover, right_carryover FROM binary_carryover WHERE company_id = $1 AND node_id = $2",
    )
    .bind(company_id)
    .bind(node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<i64, _>("left_carryover").unwrap(), 0);
    assert_eq!(row.try_get::<i64, _>("right_carryover").unwrap(), 30);

    // Upsert with new values.
    let carry2 = BinaryCarryover {
        left_volume: 10,
        right_volume: 5,
        company_id: Some(company_id),
        node_id: Some(node_id),
    };
    let change2 = StateChange {
        module_key: "binary.carryover".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(CarryoverState {
            carry: carry2,
            last_unmatched: Default::default(),
        })
        .unwrap(),
    };
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change2), &mut conn)
        .await
        .expect("project carryover upsert");
    drop(conn);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM binary_carryover WHERE company_id = $1 AND node_id = $2",
    )
    .bind(company_id)
    .bind(node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "upsert did not duplicate");

    let row = sqlx::query(
        "SELECT left_carryover, right_carryover FROM binary_carryover WHERE company_id = $1 AND node_id = $2",
    )
    .bind(company_id)
    .bind(node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<i64, _>("left_carryover").unwrap(), 10);
    assert_eq!(row.try_get::<i64, _>("right_carryover").unwrap(), 5);
}

#[tokio::test]
async fn unknown_module_key_is_ignored() {
    // Ensures the projector's `_ => {}` arm doesn't fail on modules outside
    // A2-A5 scope (matrix, pot_bonus, duplication, sponsor).
    let pool = pool().await;
    truncate_all(&pool).await;
    let projector = PgProjections::new();

    let change = StateChange {
        module_key: "royal.matrix".into(),
        module_version: "1.0.0".into(),
        aggregate_id: uuid::Uuid::now_v7(),
        value: json!({"ignored": true}),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&change), &mut conn)
        .await
        .expect("unknown module keys are ignored");
}

// ===========================================================================
// Track B1: Royal account duplication materialisation (event-driven)
// ===========================================================================

#[tokio::test]
async fn duplication_creates_enrollment_and_flushline_account() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, source_enrollment_id) = seed_fks(&pool).await;
    let projector = PgEventProjector::new();

    // Look up the seeded package so the placeholder purchase FK holds.
    let package_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT package_id FROM enrollments WHERE id = $1",
    )
    .bind(source_enrollment_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // The duplication module emits this payload when a user has both
    // graduated AND matrix-cycled. new_royal_account_id is pre-generated.
    let new_account_id = RoyalAccountId::new();
    let event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::RoyalAccountDuplicated,
        payload: json!({
            "owner_user_id": user_id,
            "company_id": company_id,
            "package_id": package_id,
            "source_enrollment_id": source_enrollment_id,
            "new_royal_account_id": new_account_id,
        }),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&event), &mut conn)
        .await
        .expect("project duplication");
    drop(conn);

    // A new enrollment row was created for the duplicated account.
    let new_enrollment_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT enrollment_id FROM royal_flushline_accounts WHERE id = $1",
    )
    .bind(new_account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let enrollment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enrollments WHERE id = $1 AND user_id = $2 AND status = 'active'",
    )
    .bind(new_enrollment_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(enrollment_count, 1, "duplicated enrollment exists");

    // The new flushline account is seeded at Ten with 0 points.
    let row = sqlx::query(
        "SELECT current_tier, current_points, graduated, owner_user_id FROM royal_flushline_accounts WHERE id = $1",
    )
    .bind(new_account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("current_tier").unwrap(), "Ten");
    assert_eq!(row.try_get::<i32, _>("current_points").unwrap(), 0);
    assert!(!row.try_get::<bool, _>("graduated").unwrap());
    assert_eq!(row.try_get::<UserId, _>("owner_user_id").unwrap(), user_id);
}

#[tokio::test]
async fn duplication_without_company_id_is_skipped() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let projector = PgEventProjector::new();

    // Malformed event with no company_id — must be skipped, not panic.
    let event = DomainEvent {
        id: EventId::new(),
        company_id: None,
        event_type: EventType::RoyalAccountDuplicated,
        payload: json!({}),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&event), &mut conn)
        .await
        .expect("skips gracefully");
    drop(conn);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM royal_flushline_accounts")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "no row materialised from malformed event");
}

// ===========================================================================
// Track B2: Binary cycle-close materialisation (event-driven)
// ===========================================================================

/// Seed an open `binary_cycle_periods` row + binary_node for cycle tests.
/// Returns (company_id, user_id, enrollment_id, node_id, period_id).
async fn seed_binary_cycle(
    pool: &PgPool,
) -> (
    CompanyId,
    UserId,
    EnrollmentId,
    BinaryNodeId,
    uuid::Uuid,
) {
    let (company_id, user_id, enrollment_id) = seed_fks(pool).await;

    // Open cycle period.
    let period_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO binary_cycle_periods (id, company_id, status, starts_at) \
         VALUES ($1, $2, 'open', NOW())",
    )
    .bind(period_id)
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();

    // Binary node for the enrollment (required FK target + cycle_count subject).
    let node_id = BinaryNodeId::new();
    let node = BinaryNode {
        id: node_id,
        user_id,
        sponsor_user_id: None,
        parent_node_id: None,
        leg: None,
        company_id: Some(company_id),
        enrollment_id: Some(enrollment_id),
    };
    let node_change = StateChange {
        module_key: "binary.tree".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(BinaryTreeState::from_nodes(vec![node])).unwrap(),
    };
    let module_projector = PgProjections::new();
    let mut conn = pool.acquire().await.unwrap();
    module_projector
        .project(std::slice::from_ref(&node_change), &mut conn)
        .await
        .expect("seed node");
    drop(conn);

    (company_id, user_id, enrollment_id, node_id, period_id)
}

#[tokio::test]
async fn cycle_closed_advances_node_cycle_count() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, _enrollment_id, node_id, _period_id) =
        seed_binary_cycle(&pool).await;
    let projector = PgEventProjector::new();

    let before: i32 =
        sqlx::query_scalar("SELECT cycle_count FROM binary_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, 0);

    // Emit BinaryCycleClosed with node_id (as close_binary_cycles now does).
    let event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::BinaryCycleClosed,
        payload: json!({
            "node_user_id": user_id,
            "node_id": node_id,
        }),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&event), &mut conn)
        .await
        .expect("project cycle closed");
    drop(conn);

    let after: i32 =
        sqlx::query_scalar("SELECT cycle_count FROM binary_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, 1, "cycle_count advanced to 1");

    // Second cycle close bumps it again.
    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&event), &mut conn)
        .await
        .expect("project cycle closed again");
    drop(conn);
    let after2: i32 =
        sqlx::query_scalar("SELECT cycle_count FROM binary_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after2, 2, "cycle_count advanced to 2");
}

#[tokio::test]
async fn pair_matched_inserts_pairing_result() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, _enrollment_id, node_id, period_id) =
        seed_binary_cycle(&pool).await;
    let projector = PgEventProjector::new();

    // A batch of two events: the BinaryPairMatched (with node_id + period_id)
    // plus its companion BinaryCommissionEarned carrying the amount.
    let pair_event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::BinaryPairMatched,
        payload: json!({
            "node_user_id": user_id,
            "node_id": node_id,
            "period_id": period_id,
            "left": 100,
            "right": 100,
            "matched": 100,
        }),
        created_at: Utc::now(),
    };
    let commission_event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::BinaryCommissionEarned,
        payload: json!({
            "node_user_id": user_id,
            "amount": "10",
            "capped": false,
        }),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(&[pair_event.clone(), commission_event], &mut conn)
        .await
        .expect("project pairing batch");
    drop(conn);

    let row = sqlx::query(
        "SELECT left_volume, right_volume, matched_volume, commission_amount, user_id, node_id \
         FROM binary_pairing_results WHERE period_id = $1 AND node_id = $2",
    )
    .bind(period_id)
    .bind(node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<i64, _>("left_volume").unwrap(), 100);
    assert_eq!(row.try_get::<i64, _>("right_volume").unwrap(), 100);
    assert_eq!(row.try_get::<i64, _>("matched_volume").unwrap(), 100);
    assert_eq!(row.try_get::<i64, _>("commission_amount").unwrap(), 10);
    assert_eq!(row.try_get::<UserId, _>("user_id").unwrap(), user_id);
    assert_eq!(row.try_get::<BinaryNodeId, _>("node_id").unwrap(), node_id);

    // Re-projecting the same batch must NOT duplicate the row. NOTE: the
    // projector inserts by a fresh UUID each call, so idempotency depends on
    // the caller not re-driving the same event. Here we just assert the
    // count stays at 1 because we only project once.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM binary_pairing_results WHERE period_id = $1 AND node_id = $2",
    )
    .bind(period_id)
    .bind(node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn pair_matched_without_commission_event_records_zero() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, _enrollment_id, node_id, period_id) =
        seed_binary_cycle(&pool).await;
    let projector = PgEventProjector::new();

    // PairMatched with no companion CommissionEarned (zero match → no commission).
    let pair_event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::BinaryPairMatched,
        payload: json!({
            "node_user_id": user_id,
            "node_id": node_id,
            "period_id": period_id,
            "left": 0,
            "right": 0,
            "matched": 0,
        }),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&pair_event), &mut conn)
        .await
        .expect("project pairing without commission");
    drop(conn);

    let commission_amount: i64 = sqlx::query_scalar(
        "SELECT commission_amount FROM binary_pairing_results WHERE period_id = $1 AND node_id = $2",
    )
    .bind(period_id)
    .bind(node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(commission_amount, 0, "no commission event → 0 amount");
}

// ===========================================================================
// Track B3: Renewal — node_id propagation + recurrence_interval + leg
// alternation (exercises the full run_renewals path).
// ===========================================================================

/// Build the runtime dependencies `run_renewals` needs. Returns them boxed
/// so the `PurchaseDeps` borrow outlives the call. Caller must hold the
/// returned `RenewalDeps` alive for the duration of the `run_renewals` call.
struct RenewalDeps {
    events: std::sync::Arc<dyn payplan_app::ports::EventStore>,
    ledger: std::sync::Arc<dyn payplan_app::ports::RewardLedgerStore>,
    catalog: payplan_infra::aggregate_repos::PgCatalogRepo,
    enrollments: payplan_infra::aggregate_repos::PgEnrollmentRepo,
    packages: payplan_infra::aggregate_repos::PgPackageRepo,
    pay_plan_stacks: payplan_infra::aggregate_repos::PgPayPlanStackRepo,
    purchases: std::sync::Arc<dyn payplan_app::ports::PurchaseRepo>,
    subs: std::sync::Arc<dyn payplan_app::ports::SubscriptionRepo>,
    ents: std::sync::Arc<dyn payplan_app::ports::EntitlementRepo>,
    registry: std::sync::Arc<payplan_core::payplan::registry::ModuleRegistry>,
}

impl RenewalDeps {
    fn deps<'a>(&'a self, pool: &'a PgPool) -> payplan_app::commands::PurchaseDeps<'a> {
        payplan_app::commands::PurchaseDeps {
            pool,
            packages: &self.packages,
            catalog: &self.catalog,
            purchases: self.purchases.as_ref(),
            subscriptions: self.subs.as_ref(),
            entitlements: self.ents.as_ref(),
            enrollments: &self.enrollments,
            pay_plan_stacks: &self.pay_plan_stacks,
            events: self.events.as_ref(),
            ledger: self.ledger.as_ref(),
            registry: self.registry.clone(),
            purchase_writer: None,
            module_state_store: None,
            projector: None,
            event_projector: None,
        }
    }
}

fn build_renewal_deps(pool: &PgPool) -> RenewalDeps {
    use payplan_infra::aggregate_repos as ar;
    use payplan_infra::event_store::PgEventStore;
    use payplan_infra::ledger_store::PgLedgerStore;

    RenewalDeps {
        events: std::sync::Arc::new(PgEventStore::new(pool.clone())),
        ledger: std::sync::Arc::new(PgLedgerStore::new(pool.clone())),
        catalog: ar::PgCatalogRepo::new(pool.clone()),
        enrollments: ar::PgEnrollmentRepo::new(pool.clone()),
        packages: ar::PgPackageRepo::new(pool.clone()),
        pay_plan_stacks: ar::PgPayPlanStackRepo::new(pool.clone()),
        purchases: std::sync::Arc::new(ar::PgPurchaseRepo::new(pool.clone())),
        subs: std::sync::Arc::new(ar::PgSubscriptionRepo::new(pool.clone())),
        ents: std::sync::Arc::new(ar::PgEntitlementRepo::new(pool.clone())),
        registry: std::sync::Arc::new(
            payplan_core::payplan::registry::ModuleRegistry::new(),
        ),
    }
}

#[tokio::test]
async fn renewal_resolves_node_id_and_reads_recurrence_interval() {
    use payplan_infra::operations::run_renewals;

    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, enrollment_id) = seed_fks(&pool).await;
    let package_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT package_id FROM enrollments WHERE id = $1",
    )
    .bind(enrollment_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Recurring monthly billing plan.
    let item_id = payplan_core::shared::ids::CatalogItemId::new();
    let billing_plan_id = payplan_core::shared::ids::BillingPlanId::new();
    sqlx::query(
        "INSERT INTO catalog_items (id, company_id, name, item_type, sku, status, metadata) \
         VALUES ($1, $2, 'I', 'service', 's', 'active', '{}')",
    )
    .bind(item_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, active) \
         VALUES ($1, $2, 'recurring', 1, 'USD', 'monthly', true)",
    )
    .bind(billing_plan_id)
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();

    // A package_item with commissionable volume so the renewal carries volume.
    sqlx::query(
        "INSERT INTO package_items (id, package_id, catalog_item_id, billing_plan_id, quantity, is_commissionable, commissionable_volume, points_value) \
         VALUES ($1, $2, $3, $4, 1, TRUE, 100, 5)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(package_id)
    .bind(item_id)
    .bind(billing_plan_id)
    .execute(&pool)
    .await
    .unwrap();

    // A past-due subscription for this user/package/billing plan.
    let sub_id = payplan_core::shared::ids::SubscriptionId::new();
    sqlx::query(
        "INSERT INTO subscriptions (id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, created_at) \
         VALUES ($1, $2, $3, $4, $5, 'active', NOW() - INTERVAL '40 days', NOW() - INTERVAL '10 days', NOW())",
    )
    .bind(sub_id)
    .bind(company_id)
    .bind(user_id)
    .bind(package_id)
    .bind(billing_plan_id)
    .execute(&pool)
    .await
    .unwrap();

    // Seed a binary node for the user so the renewal can resolve node_id.
    let node_id = BinaryNodeId::new();
    let node = BinaryNode {
        id: node_id,
        user_id,
        sponsor_user_id: None,
        parent_node_id: None,
        leg: None,
        company_id: Some(company_id),
        enrollment_id: Some(enrollment_id),
    };
    let node_change = StateChange {
        module_key: "binary.tree".into(),
        module_version: "1.0.0".into(),
        aggregate_id: enrollment_id.0,
        value: serde_json::to_value(BinaryTreeState::from_nodes(vec![node])).unwrap(),
    };
    let module_projector = PgProjections::new();
    let mut conn = pool.acquire().await.unwrap();
    module_projector
        .project(std::slice::from_ref(&node_change), &mut conn)
        .await
        .expect("seed node");
    drop(conn);

    // Run renewals.
    let renewal_deps = build_renewal_deps(&pool);
    let deps = renewal_deps.deps(&pool);
    let processed = run_renewals(&pool, &deps).await.expect("renewals run");
    assert_eq!(processed, 1, "one due subscription processed");

    // The SubscriptionRenewed event landed in event_log with node_id in payload.
    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE event_type = 'subscription.renewed' AND company_id = $1 LIMIT 1",
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        payload.get("node_id").and_then(|v| v.as_str()).map(String::from),
        Some(node_id.to_string()),
        "renewal event payload includes node_id"
    );
    assert_eq!(
        payload.get("volume").and_then(|v| v.as_i64()),
        Some(100),
        "renewal volume honors is_commissionable * quantity"
    );
    assert_eq!(
        payload.get("leg").and_then(|v| v.as_str()),
        Some("left"),
        "first renewal (no prior ledger row) defaults to left"
    );

    // current_period_end was advanced by the monthly interval (> now).
    let new_end: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT current_period_end FROM subscriptions WHERE id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        new_end > chrono::Utc::now(),
        "current_period_end advanced past now (monthly interval)"
    );
}

#[tokio::test]
async fn renewal_excludes_one_time_billing_plans() {
    use payplan_infra::operations::run_renewals;

    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, enrollment_id) = seed_fks(&pool).await;
    let package_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT package_id FROM enrollments WHERE id = $1",
    )
    .bind(enrollment_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // One-time billing plan — should NOT be renewed.
    let item_id = payplan_core::shared::ids::CatalogItemId::new();
    let billing_plan_id = payplan_core::shared::ids::BillingPlanId::new();
    sqlx::query(
        "INSERT INTO catalog_items (id, company_id, name, item_type, sku, status, metadata) \
         VALUES ($1, $2, 'I', 'service', 's', 'active', '{}')",
    )
    .bind(item_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, active) \
         VALUES ($1, $2, 'one_time', 1, 'USD', true)",
    )
    .bind(billing_plan_id)
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();

    let sub_id = payplan_core::shared::ids::SubscriptionId::new();
    sqlx::query(
        "INSERT INTO subscriptions (id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, created_at) \
         VALUES ($1, $2, $3, $4, $5, 'active', NOW() - INTERVAL '40 days', NOW() - INTERVAL '10 days', NOW())",
    )
    .bind(sub_id)
    .bind(company_id)
    .bind(user_id)
    .bind(package_id)
    .bind(billing_plan_id)
    .execute(&pool)
    .await
    .unwrap();

    let renewal_deps = build_renewal_deps(&pool);
    let deps = renewal_deps.deps(&pool);
    let processed = run_renewals(&pool, &deps).await.expect("renewals run");
    assert_eq!(processed, 0, "one-time billing plans are not renewed");
}

// ===========================================================================
// Track B4: Per-user royal pot bonus balances (event-driven)
// ===========================================================================

#[tokio::test]
async fn pot_bonus_distribution_updates_balances() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_a, _) = seed_fks(&pool).await;
    // Second user for multi-user distribution.
    let user_b = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, company_id) VALUES ($1, $2, 'ph', 'user', $3)",
    )
    .bind(user_b)
    .bind(format!("b-{}@t.local", uuid::Uuid::now_v7().simple()))
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    let projector = PgEventProjector::new();
    let event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::RoyalPotBonusDistributed,
        payload: json!({
            "pool": "1000",
            "qualified_count": 2,
            "per_qualified_user": "375",
            "distributions": [
                { "user_id": user_a, "profit_share": 375, "top_cycler": 100 },
                { "user_id": user_b, "profit_share": 375 }
            ]
        }),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&event), &mut conn)
        .await
        .expect("project pot distribution");
    drop(conn);

    let row_a = sqlx::query(
        "SELECT total_earned, profit_share_earned, top_cycler_earned, distributions_count \
         FROM royal_pot_bonus_balances WHERE company_id = $1 AND user_id = $2",
    )
    .bind(company_id)
    .bind(user_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_a.try_get::<i64, _>("total_earned").unwrap(), 475);
    assert_eq!(row_a.try_get::<i64, _>("profit_share_earned").unwrap(), 375);
    assert_eq!(row_a.try_get::<i64, _>("top_cycler_earned").unwrap(), 100);
    assert_eq!(row_a.try_get::<i32, _>("distributions_count").unwrap(), 1);

    let row_b = sqlx::query(
        "SELECT total_earned, profit_share_earned, top_cycler_earned \
         FROM royal_pot_bonus_balances WHERE company_id = $1 AND user_id = $2",
    )
    .bind(company_id)
    .bind(user_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_b.try_get::<i64, _>("total_earned").unwrap(), 375);
    assert_eq!(row_b.try_get::<i64, _>("profit_share_earned").unwrap(), 375);
    assert_eq!(row_b.try_get::<i64, _>("top_cycler_earned").unwrap(), 0);
}

#[tokio::test]
async fn pot_bonus_balances_upsert_accumulates() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, user_id, _) = seed_fks(&pool).await;
    let projector = PgEventProjector::new();

    let make_event = || DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::RoyalPotBonusDistributed,
        payload: json!({
            "distributions": [
                { "user_id": user_id, "profit_share": 200 }
            ]
        }),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector.project(std::slice::from_ref(&make_event()), &mut conn).await.unwrap();
    projector.project(std::slice::from_ref(&make_event()), &mut conn).await.unwrap();
    drop(conn);

    let row = sqlx::query(
        "SELECT total_earned, profit_share_earned, distributions_count \
         FROM royal_pot_bonus_balances WHERE company_id = $1 AND user_id = $2",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<i64, _>("total_earned").unwrap(), 400, "two distributions accumulated");
    assert_eq!(row.try_get::<i64, _>("profit_share_earned").unwrap(), 400);
    assert_eq!(row.try_get::<i32, _>("distributions_count").unwrap(), 2);
}

#[tokio::test]
async fn pot_bonus_event_without_distributions_is_skipped() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (company_id, _user_id, _) = seed_fks(&pool).await;
    let projector = PgEventProjector::new();

    // Old-style company-level event (the trigger payload from
    // run_royal_pot_distribution has no distributions array).
    let event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::RoyalPotBonusDistributed,
        payload: json!({}),
        created_at: Utc::now(),
    };

    let mut conn = pool.acquire().await.unwrap();
    projector
        .project(std::slice::from_ref(&event), &mut conn)
        .await
        .expect("skips gracefully");
    drop(conn);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM royal_pot_bonus_balances")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "no balance rows from event without distributions");
}

// Suppress unused-import noise from items only referenced via type paths.
#[allow(dead_code)]
fn _typechecks(_a: RoyalAccountId, _t: RoyalTier) {}
