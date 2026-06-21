-- Task 11: store exact Decimal commissions instead of truncating to minor
-- units. The event payload carries Decimal amounts (e.g. 187.5), but these
-- columns were BIGINT, so the projector truncated toward zero (187.5 -> 187)
-- and the error accumulated across cycles via the cumulative upsert. Widen the
-- fractional money columns to NUMERIC(20,4) (the project's money convention) so
-- the relational projection matches the canonical ledger exactly.

ALTER TABLE royal_pot_bonus_balances
    ALTER COLUMN total_earned        TYPE NUMERIC(20,4),
    ALTER COLUMN profit_share_earned TYPE NUMERIC(20,4),
    ALTER COLUMN top_cycler_earned   TYPE NUMERIC(20,4);

ALTER TABLE binary_pairing_results
    ALTER COLUMN commission_amount TYPE NUMERIC(20,4);
