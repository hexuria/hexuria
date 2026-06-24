-- Dev seed for the PayPlan Platform.
--
-- Inserts two packages: a Royal Flush package and a Binary package.
-- Idempotent for re-runs.
--
-- Apply with:
--   psql "$DATABASE_URL" -f seeds/dev.sql

BEGIN;

DELETE FROM reward_ledger;
DELETE FROM event_log;
DELETE FROM entitlements;
DELETE FROM enrollments;
DELETE FROM purchases;
DELETE FROM subscriptions;
DELETE FROM package_items;
DELETE FROM packages;
DELETE FROM product_payplan_allocations;
DELETE FROM pay_plan_stack_modules;
DELETE FROM pay_plan_stacks;
DELETE FROM billing_plans;
DELETE FROM catalog_items;
DELETE FROM users WHERE email IN ('admin@payplan.com', 'user@payplan.com');

-- Stable IDs so re-runs don't churn UUIDs.
DO $$
DECLARE
    item_rfn_id CONSTANT UUID := '22222222-2222-2222-2222-222222222221';
    item_rfn_billing_id CONSTANT UUID := '33333333-3333-3333-3333-333333333331';

    item_binary_id CONSTANT UUID := '22222222-2222-2222-2222-222222222222';
    item_binary_billing_id CONSTANT UUID := '33333333-3333-3333-3333-333333333332';

    stack_rfn_id CONSTANT UUID := '44444444-4444-4444-4444-444444444441';
    stack_binary_id CONSTANT UUID := '44444444-4444-4444-4444-444444444442';

    pkg_rfn_id CONSTANT UUID := '55555555-5555-5555-5555-555555555551';
    pkg_binary_id CONSTANT UUID := '55555555-5555-5555-5555-555555555552';

    admin_id CONSTANT UUID := 'a1111111-1111-1111-1111-111111111111';
    user_id CONSTANT UUID := 'a1111111-1111-1111-1111-333333333333';
BEGIN
    INSERT INTO catalog_items (id, name, description, item_type, sku, status, metadata)
    VALUES
        (item_rfn_id, 'Royal Flush Training Membership', 'Monthly training membership for the Royal Flush stack', 'service', 'RFN-MEM', 'active', '{}'),
        (item_binary_id, 'Binary Builder Software Subscription', 'Monthly access to the binary builder software', 'service', 'BIN-SUB', 'active', '{}');

    INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, recurrence_count, trial_days, grace_period_days, active)
    VALUES
        (item_rfn_billing_id, item_rfn_id, 'recurring', 99.00, 'USD', 'monthly', 1, 0, 7, TRUE),
        (item_binary_billing_id, item_binary_id, 'recurring', 149.00, 'USD', 'monthly', 1, 0, 7, TRUE);

    INSERT INTO pay_plan_stacks (id, name, version, status)
    VALUES
        (stack_rfn_id, 'Royal Flush Stack', 1, 'active'),
        (stack_binary_id, 'Binary Stack', 1, 'active');

    INSERT INTO pay_plan_stack_modules (id, stack_id, module_key, module_version, sort_order, config, active) VALUES
        (gen_random_uuid(), stack_rfn_id, 'sponsor.allocation', '1.0.0', 10, '{}', TRUE),
        (gen_random_uuid(), stack_rfn_id, 'royal.flushline', '1.0.0', 20, '{}', TRUE),
        (gen_random_uuid(), stack_rfn_id, 'royal.matrix', '1.0.0', 30, '{}', TRUE),
        (gen_random_uuid(), stack_rfn_id, 'royal.pot_bonus', '1.0.0', 40, '{}', TRUE),
        (gen_random_uuid(), stack_rfn_id, 'royal.account_duplication', '1.0.0', 50, '{}', TRUE),
        (gen_random_uuid(), stack_binary_id, 'sponsor.allocation', '1.0.0', 10, '{}', TRUE),
        (gen_random_uuid(), stack_binary_id, 'binary.tree', '1.0.0', 20, '{"strategy":"auto_balance"}', TRUE),
        (gen_random_uuid(), stack_binary_id, 'binary.volume', '1.0.0', 30, '{"count_purchase_volume":true,"count_renewal_volume":true,"carryover_enabled":true}', TRUE),
        (gen_random_uuid(), stack_binary_id, 'binary.pairing_bonus', '1.0.0', 40, '{"left_ratio":1,"right_ratio":1,"commission_percent":10}', TRUE),
        (gen_random_uuid(), stack_binary_id, 'binary.carryover', '1.0.0', 50, '{}', TRUE);

    INSERT INTO product_payplan_allocations (id, catalog_item_id, pay_plan_stack_id, points, active)
    VALUES
        (gen_random_uuid(), item_rfn_id, stack_rfn_id, 5, TRUE),
        (gen_random_uuid(), item_binary_id, stack_binary_id, 100, TRUE);

    INSERT INTO packages (id, name, description, status, metadata)
    VALUES
        (pkg_rfn_id, 'Royal Flush Starter Membership', 'Monthly recurring membership for the Royal Flush stack', 'active', '{}'),
        (pkg_binary_id, 'Binary Builder Premium', 'Monthly software subscription for the Binary stack', 'active', '{}');

    INSERT INTO package_items (id, package_id, catalog_item_id, billing_plan_id, quantity, item_role, is_commissionable)
    VALUES
        (gen_random_uuid(), pkg_rfn_id, item_rfn_id, item_rfn_billing_id, 1, 'included', TRUE),
        (gen_random_uuid(), pkg_binary_id, item_binary_id, item_binary_billing_id, 1, 'included', TRUE);

    -- Seeded Users
    -- 1. Admin (no company_id, role is admin)
    INSERT INTO users (id, email, password_hash, email_verified, role)
    VALUES (admin_id, 'admin@payplan.com', '$argon2id$v=19$m=19456,t=2,p=1$hyoYi8zQETUbfOHWBQJuGg$fH1zPmKTIPBQh3i56bzxpk0T4fGLI2JNPyz3RTD4fL0', TRUE, 'admin');

    -- 2. Regular User (role is user)
    INSERT INTO users (id, email, password_hash, email_verified, role)
    VALUES (user_id, 'user@payplan.com', '$argon2id$v=19$m=19456,t=2,p=1$hyoYi8zQETUbfOHWBQJuGg$fH1zPmKTIPBQh3i56bzxpk0T4fGLI2JNPyz3RTD4fL0', TRUE, 'user');
END $$;

COMMIT;

-- Sanity summary.
SELECT
    (SELECT COUNT(*) FROM catalog_items) AS catalog_items,
    (SELECT COUNT(*) FROM billing_plans) AS billing_plans,
    (SELECT COUNT(*) FROM pay_plan_stacks) AS stacks,
    (SELECT COUNT(*) FROM pay_plan_stack_modules) AS stack_modules,
    (SELECT COUNT(*) FROM packages) AS packages,
    (SELECT COUNT(*) FROM product_payplan_allocations) AS allocations,
    (SELECT COUNT(*) FROM users) AS users;
