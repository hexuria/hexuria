-- Drop amount and currency columns from reward_ledger
ALTER TABLE reward_ledger DROP COLUMN IF EXISTS amount;
ALTER TABLE reward_ledger DROP COLUMN IF EXISTS currency;

-- Update binary_pairing_results to use points instead of commission_amount
ALTER TABLE binary_pairing_results DROP COLUMN IF EXISTS commission_amount;
ALTER TABLE binary_pairing_results ADD COLUMN points BIGINT NOT NULL DEFAULT 0;

-- Convert royal_pot_bonus_balances columns back to BIGINT for point values
ALTER TABLE royal_pot_bonus_balances ALTER COLUMN total_earned TYPE BIGINT USING total_earned::BIGINT;
ALTER TABLE royal_pot_bonus_balances ALTER COLUMN profit_share_earned TYPE BIGINT USING profit_share_earned::BIGINT;
ALTER TABLE royal_pot_bonus_balances ALTER COLUMN top_cycler_earned TYPE BIGINT USING top_cycler_earned::BIGINT;
