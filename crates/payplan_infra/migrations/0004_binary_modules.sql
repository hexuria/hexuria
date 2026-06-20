CREATE TABLE binary_nodes (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    enrollment_id UUID NOT NULL REFERENCES enrollments(id),
    user_id UUID NOT NULL REFERENCES users(id),
    sponsor_user_id UUID REFERENCES users(id),
    parent_node_id UUID REFERENCES binary_nodes(id),
    leg TEXT CHECK (leg IN ('left', 'right')),
    placed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(parent_node_id, leg)
);

CREATE TABLE binary_volume_ledger (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    node_id UUID NOT NULL REFERENCES binary_nodes(id),
    source_purchase_id UUID REFERENCES purchases(id),
    leg TEXT NOT NULL CHECK (leg IN ('left', 'right')),
    volume BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE binary_cycle_periods (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    status TEXT NOT NULL DEFAULT 'open',
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ
);

CREATE TABLE binary_pairing_results (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    period_id UUID NOT NULL REFERENCES binary_cycle_periods(id),
    user_id UUID NOT NULL REFERENCES users(id),
    node_id UUID NOT NULL REFERENCES binary_nodes(id),
    left_volume BIGINT NOT NULL,
    right_volume BIGINT NOT NULL,
    matched_volume BIGINT NOT NULL,
    commission_amount BIGINT NOT NULL,
    ledger_entry_id UUID REFERENCES reward_ledger(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE binary_carryover (
    company_id UUID NOT NULL REFERENCES companies(id),
    node_id UUID NOT NULL REFERENCES binary_nodes(id),
    left_carryover BIGINT NOT NULL DEFAULT 0,
    right_carryover BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(company_id, node_id)
);
