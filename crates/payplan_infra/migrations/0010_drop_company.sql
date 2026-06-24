-- Drop company-related indexes
DROP INDEX IF EXISTS users_company_idx;
DROP INDEX IF EXISTS catalog_items_company_idx;
DROP INDEX IF EXISTS packages_company_idx;
DROP INDEX IF EXISTS reward_ledger_company_idx;
DROP INDEX IF EXISTS event_log_company_type_time_idx;
DROP INDEX IF EXISTS royal_flushline_accounts_company_idx;
DROP INDEX IF EXISTS royal_pot_bonus_balances_company_idx;

-- Alter users
ALTER TABLE users DROP COLUMN IF EXISTS company_id;

-- Alter catalog_items
ALTER TABLE catalog_items DROP COLUMN IF EXISTS company_id;

-- Alter pay_plan_stacks
ALTER TABLE pay_plan_stacks DROP CONSTRAINT IF EXISTS pay_plan_stacks_company_id_name_version_key;
ALTER TABLE pay_plan_stacks DROP COLUMN IF EXISTS company_id;
ALTER TABLE pay_plan_stacks ADD CONSTRAINT pay_plan_stacks_name_version_key UNIQUE (name, version);

-- Alter packages
ALTER TABLE packages DROP COLUMN IF EXISTS company_id;

-- Alter purchases
ALTER TABLE purchases DROP COLUMN IF EXISTS company_id;

-- Alter subscriptions
ALTER TABLE subscriptions DROP COLUMN IF EXISTS company_id;

-- Alter entitlements
ALTER TABLE entitlements DROP COLUMN IF EXISTS company_id;

-- Alter enrollments
ALTER TABLE enrollments DROP COLUMN IF EXISTS company_id;

-- Alter event_log
ALTER TABLE event_log DROP COLUMN IF EXISTS company_id;
CREATE INDEX event_log_type_time_idx ON event_log (event_type, created_at DESC);

-- Alter reward_ledger
ALTER TABLE reward_ledger DROP COLUMN IF EXISTS company_id;

-- Alter royal_flushline_accounts
ALTER TABLE royal_flushline_accounts DROP COLUMN IF EXISTS company_id;

-- Alter royal_matrices
ALTER TABLE royal_matrices DROP COLUMN IF EXISTS company_id;

-- Alter royal_qualifications
ALTER TABLE royal_qualifications DROP CONSTRAINT IF EXISTS royal_qualifications_pkey;
ALTER TABLE royal_qualifications DROP COLUMN IF EXISTS company_id;
ALTER TABLE royal_qualifications ADD PRIMARY KEY (user_id);

-- Alter royal_pot_bonus_pool (recreate without company_id PK)
DROP TABLE IF EXISTS royal_pot_bonus_pool;
CREATE TABLE royal_pot_bonus_pool (
    id UUID PRIMARY KEY,
    total_pool_points BIGINT NOT NULL DEFAULT 0,
    last_distribution_at TIMESTAMPTZ
);

-- Alter royal_pot_bonus_balances
ALTER TABLE royal_pot_bonus_balances DROP CONSTRAINT IF EXISTS royal_pot_bonus_balances_pkey;
ALTER TABLE royal_pot_bonus_balances DROP COLUMN IF EXISTS company_id;
ALTER TABLE royal_pot_bonus_balances ADD PRIMARY KEY (user_id);

-- Alter binary_nodes
ALTER TABLE binary_nodes DROP COLUMN IF EXISTS company_id;

-- Alter binary_volume_ledger
ALTER TABLE binary_volume_ledger DROP COLUMN IF EXISTS company_id;

-- Alter binary_cycle_periods
ALTER TABLE binary_cycle_periods DROP COLUMN IF EXISTS company_id;

-- Alter binary_pairing_results
ALTER TABLE binary_pairing_results DROP COLUMN IF EXISTS company_id;

-- Alter binary_carryover
ALTER TABLE binary_carryover DROP CONSTRAINT IF EXISTS binary_carryover_pkey;
ALTER TABLE binary_carryover DROP COLUMN IF EXISTS company_id;
ALTER TABLE binary_carryover ADD PRIMARY KEY (node_id);

-- Drop the companies table
DROP TABLE IF EXISTS companies CASCADE;
