-- Track B4: per-user cumulative royal pot bonus balances. One row per
-- (company, user); totals accumulate on each RoyalPotBonusDistributed event
-- via the PgEventProjector (cumulative upsert).
CREATE TABLE royal_pot_bonus_balances (
    company_id          UUID NOT NULL REFERENCES companies(id),
    user_id             UUID NOT NULL REFERENCES users(id),
    total_earned        BIGINT NOT NULL DEFAULT 0,
    profit_share_earned BIGINT NOT NULL DEFAULT 0,
    top_cycler_earned   BIGINT NOT NULL DEFAULT 0,
    distributions_count INTEGER NOT NULL DEFAULT 0,
    last_distribution_at TIMESTAMPTZ,
    PRIMARY KEY (company_id, user_id)
);

CREATE INDEX royal_pot_bonus_balances_company_idx ON royal_pot_bonus_balances(company_id);
