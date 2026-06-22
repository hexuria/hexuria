-- Dev seed for the PayPlan Platform.
--
-- Inserts one company ("Acme") with two packages: a Royal Flush package and
-- a Binary package. Idempotent for re-runs (clears Acme's data first).
--
-- Apply with:
--   psql "$DATABASE_URL" -f seeds/dev.sql

BEGIN;

DELETE FROM reward_ledger WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM event_log WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM entitlements WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM enrollments WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM purchases WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM subscriptions WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM package_items WHERE package_id IN (SELECT id FROM packages WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme'));
DELETE FROM packages WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM pay_plan_stack_modules WHERE stack_id IN (SELECT id FROM pay_plan_stacks WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme'));
DELETE FROM pay_plan_stacks WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM billing_plans WHERE catalog_item_id IN (SELECT id FROM catalog_items WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme'));
DELETE FROM catalog_items WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme');
DELETE FROM users WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme') OR email IN ('admin@payplan.com', 'acme_admin@payplan.com', 'user@payplan.com');
DELETE FROM companies WHERE slug = 'acme';

-- Stable IDs so re-runs don't churn UUIDs.
DO $$
DECLARE
    company_id CONSTANT UUID := '11111111-1111-1111-1111-111111111111';

    item_rfn_id CONSTANT UUID := '22222222-2222-2222-2222-222222222221';
    item_rfn_billing_id CONSTANT UUID := '33333333-3333-3333-3333-333333333331';

    item_binary_id CONSTANT UUID := '22222222-2222-2222-2222-222222222222';
    item_binary_billing_id CONSTANT UUID := '33333333-3333-3333-3333-333333333332';

    stack_rfn_id CONSTANT UUID := '44444444-4444-4444-4444-444444444441';
    stack_binary_id CONSTANT UUID := '44444444-4444-4444-4444-444444444442';

    pkg_rfn_id CONSTANT UUID := '55555555-5555-5555-5555-555555555551';
    pkg_binary_id CONSTANT UUID := '55555555-5555-5555-5555-555555555552';

    admin_id CONSTANT UUID := 'a1111111-1111-1111-1111-111111111111';
    acme_admin_id CONSTANT UUID := 'a1111111-1111-1111-1111-222222222222';
    user_id CONSTANT UUID := 'a1111111-1111-1111-1111-333333333333';
BEGIN
    INSERT INTO companies (id, name, slug, status, settings)
    VALUES (company_id, 'Acme MLM', 'acme', 'active', '{"timezone":"America/Los_Angeles"}');

    INSERT INTO catalog_items (id, company_id, name, description, item_type, sku, status, metadata)
    VALUES
        (item_rfn_id, company_id, 'Royal Flush Training Membership', 'Monthly training membership for the Royal Flush stack', 'service', 'RFN-MEM', 'active', '{}'),
        (item_binary_id, company_id, 'Binary Builder Software Subscription', 'Monthly access to the binary builder software', 'service', 'BIN-SUB', 'active', '{}');

    INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, recurrence_count, trial_days, grace_period_days, active)
    VALUES
        (item_rfn_billing_id, item_rfn_id, 'recurring', 99.00, 'USD', 'monthly', 1, 0, 7, TRUE),
        (item_binary_billing_id, item_binary_id, 'recurring', 149.00, 'USD', 'monthly', 1, 0, 7, TRUE);

    INSERT INTO pay_plan_stacks (id, company_id, name, version, status)
    VALUES
        (stack_rfn_id, company_id, 'Royal Flush Stack', 1, 'active'),
        (stack_binary_id, company_id, 'Binary Stack', 1, 'active');

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

    INSERT INTO packages (id, company_id, pay_plan_stack_id, name, description, status, metadata)
    VALUES
        (pkg_rfn_id, company_id, stack_rfn_id, 'Royal Flush Starter Membership', 'Monthly recurring membership for the Royal Flush stack', 'active', '{}'),
        (pkg_binary_id, company_id, stack_binary_id, 'Binary Builder Premium', 'Monthly software subscription for the Binary stack', 'active', '{}');

    INSERT INTO package_items (id, package_id, catalog_item_id, billing_plan_id, quantity, item_role, is_commissionable, commissionable_volume, points_value)
    VALUES
        (gen_random_uuid(), pkg_rfn_id, item_rfn_id, item_rfn_billing_id, 1, 'included', TRUE, 50, 5),
        (gen_random_uuid(), pkg_binary_id, item_binary_id, item_binary_billing_id, 1, 'included', TRUE, 100, 0);

    -- Seeded Users
    -- 1. Platform Admin (no company_id, role is platform_admin)
    INSERT INTO users (id, email, password_hash, email_verified, role, company_id)
    VALUES (admin_id, 'admin@payplan.com', '$argon2id$v=19$m=19456,t=2,p=1$hyoYi8zQETUbfOHWBQJuGg$fH1zPmKTIPBQh3i56bzxpk0T4fGLI2JNPyz3RTD4fL0', TRUE, 'platform_admin', NULL);

    -- 2. Company Admin for Acme (company_id = company_id, role is company_admin)
    INSERT INTO users (id, email, password_hash, email_verified, role, company_id)
    VALUES (acme_admin_id, 'acme_admin@payplan.com', '$argon2id$v=19$m=19456,t=2,p=1$hyoYi8zQETUbfOHWBQJuGg$fH1zPmKTIPBQh3i56bzxpk0T4fGLI2JNPyz3RTD4fL0', TRUE, 'company_admin', company_id);

    -- 3. Regular User for Acme (company_id = company_id, role is user)
    INSERT INTO users (id, email, password_hash, email_verified, role, company_id)
    VALUES (user_id, 'user@payplan.com', '$argon2id$v=19$m=19456,t=2,p=1$hyoYi8zQETUbfOHWBQJuGg$fH1zPmKTIPBQh3i56bzxpk0T4fGLI2JNPyz3RTD4fL0', TRUE, 'user', company_id);
END $$;

COMMIT;

-- Sanity summary.
SELECT
    (SELECT COUNT(*) FROM companies WHERE slug = 'acme')            AS companies,
    (SELECT COUNT(*) FROM catalog_items WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme')) AS catalog_items,
    (SELECT COUNT(*) FROM billing_plans WHERE catalog_item_id IN (SELECT id FROM catalog_items WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme'))) AS billing_plans,
    (SELECT COUNT(*) FROM pay_plan_stacks WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme')) AS stacks,
    (SELECT COUNT(*) FROM pay_plan_stack_modules)                   AS stack_modules,
    (SELECT COUNT(*) FROM packages WHERE company_id IN (SELECT id FROM companies WHERE slug = 'acme')) AS packages,
    (SELECT COUNT(*) FROM users)                                    AS users;
