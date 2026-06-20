CREATE TABLE royal_flushline_accounts (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    enrollment_id UUID NOT NULL REFERENCES enrollments(id),
    owner_user_id UUID NOT NULL REFERENCES users(id),
    current_tier TEXT,
    current_points INTEGER NOT NULL DEFAULT 0,
    cycle_count INTEGER NOT NULL DEFAULT 0,
    graduated BOOLEAN NOT NULL DEFAULT FALSE,
    graduated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX royal_flushline_accounts_company_idx ON royal_flushline_accounts(company_id);
CREATE INDEX royal_flushline_accounts_tier_idx ON royal_flushline_accounts(current_tier) WHERE graduated = FALSE;

CREATE TABLE royal_matrices (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    owner_account_id UUID NOT NULL REFERENCES royal_flushline_accounts(id),
    status TEXT NOT NULL DEFAULT 'filling',
    cycle_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    cycled_at TIMESTAMPTZ
);

CREATE TABLE royal_matrix_slots (
    matrix_id UUID NOT NULL REFERENCES royal_matrices(id) ON DELETE CASCADE,
    slot_number SMALLINT NOT NULL CHECK (slot_number BETWEEN 1 AND 7),
    account_id UUID NOT NULL REFERENCES royal_flushline_accounts(id),
    PRIMARY KEY(matrix_id, slot_number)
);

CREATE TABLE royal_qualifications (
    company_id UUID NOT NULL REFERENCES companies(id),
    user_id UUID NOT NULL REFERENCES users(id),
    total_graduations INTEGER NOT NULL DEFAULT 0,
    total_matrix_cycles INTEGER NOT NULL DEFAULT 0,
    is_qualified BOOLEAN NOT NULL DEFAULT FALSE,
    first_graduation_at TIMESTAMPTZ,
    last_cycle_at TIMESTAMPTZ,
    PRIMARY KEY(company_id, user_id)
);

CREATE TABLE royal_pot_bonus_pool (
    company_id UUID PRIMARY KEY REFERENCES companies(id),
    total_pool_points BIGINT NOT NULL DEFAULT 0,
    last_distribution_at TIMESTAMPTZ
);
