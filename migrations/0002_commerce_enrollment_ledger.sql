CREATE TABLE purchases (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    user_id UUID NOT NULL REFERENCES users(id),
    package_id UUID NOT NULL REFERENCES packages(id),
    sponsor_user_id UUID REFERENCES users(id),
    gross_amount NUMERIC(20, 4) NOT NULL,
    net_amount NUMERIC(20, 4) NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    status TEXT NOT NULL DEFAULT 'pending',
    purchased_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX purchases_user_idx ON purchases(user_id);
CREATE INDEX purchases_package_idx ON purchases(package_id);

CREATE TABLE subscriptions (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    user_id UUID NOT NULL REFERENCES users(id),
    package_id UUID NOT NULL REFERENCES packages(id),
    billing_plan_id UUID NOT NULL REFERENCES billing_plans(id),
    status TEXT NOT NULL DEFAULT 'active',
    current_period_start TIMESTAMPTZ,
    current_period_end TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX subscriptions_user_status_idx ON subscriptions(user_id, status);

CREATE TABLE entitlements (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    user_id UUID NOT NULL REFERENCES users(id),
    package_id UUID NOT NULL REFERENCES packages(id),
    catalog_item_id UUID NOT NULL REFERENCES catalog_items(id),
    source_purchase_id UUID REFERENCES purchases(id),
    source_subscription_id UUID REFERENCES subscriptions(id),
    status TEXT NOT NULL DEFAULT 'active',
    starts_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ends_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX entitlements_user_idx ON entitlements(user_id, status);

CREATE TABLE enrollments (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    user_id UUID NOT NULL REFERENCES users(id),
    package_id UUID NOT NULL REFERENCES packages(id),
    purchase_id UUID NOT NULL REFERENCES purchases(id),
    sponsor_user_id UUID REFERENCES users(id),
    status TEXT NOT NULL DEFAULT 'active',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX enrollments_user_status_idx ON enrollments(user_id, status);
CREATE INDEX enrollments_package_idx ON enrollments(package_id);

CREATE TABLE event_log (
    id UUID PRIMARY KEY,
    company_id UUID REFERENCES companies(id),
    event_type TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX event_log_company_type_time_idx ON event_log(company_id, event_type, created_at DESC);

CREATE TABLE reward_ledger (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    user_id UUID NOT NULL REFERENCES users(id),
    enrollment_id UUID REFERENCES enrollments(id),
    package_id UUID REFERENCES packages(id),
    source_module TEXT NOT NULL,
    source_event_id UUID REFERENCES event_log(id),
    amount NUMERIC(20, 4) NOT NULL DEFAULT 0,
    points BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'POINTS',
    status TEXT NOT NULL DEFAULT 'pending',
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX reward_ledger_user_status_idx ON reward_ledger(user_id, status);
CREATE INDEX reward_ledger_company_idx ON reward_ledger(company_id);
