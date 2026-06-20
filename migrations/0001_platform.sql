CREATE TABLE companies (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    role TEXT NOT NULL DEFAULT 'user',
    company_id UUID REFERENCES companies(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX users_company_idx ON users(company_id);

CREATE TABLE catalog_items (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    name TEXT NOT NULL,
    description TEXT,
    item_type TEXT NOT NULL CHECK (item_type IN ('product', 'service')),
    sku TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX catalog_items_company_idx ON catalog_items(company_id);

CREATE TABLE billing_plans (
    id UUID PRIMARY KEY,
    catalog_item_id UUID NOT NULL REFERENCES catalog_items(id),
    billing_type TEXT NOT NULL CHECK (billing_type IN ('one_time', 'recurring')),
    price_amount NUMERIC(20, 4) NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    recurrence_interval TEXT,
    recurrence_count INTEGER,
    trial_days INTEGER NOT NULL DEFAULT 0,
    grace_period_days INTEGER NOT NULL DEFAULT 0,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX billing_plans_catalog_idx ON billing_plans(catalog_item_id);

CREATE TABLE pay_plan_stacks (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    name TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(company_id, name, version)
);

CREATE TABLE pay_plan_stack_modules (
    id UUID PRIMARY KEY,
    stack_id UUID NOT NULL REFERENCES pay_plan_stacks(id) ON DELETE CASCADE,
    module_key TEXT NOT NULL,
    module_version TEXT NOT NULL DEFAULT '1',
    sort_order INTEGER NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE(stack_id, sort_order)
);

CREATE TABLE packages (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL REFERENCES companies(id),
    pay_plan_stack_id UUID REFERENCES pay_plan_stacks(id),
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX packages_company_idx ON packages(company_id);

CREATE TABLE package_items (
    id UUID PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    catalog_item_id UUID NOT NULL REFERENCES catalog_items(id),
    billing_plan_id UUID NOT NULL REFERENCES billing_plans(id),
    quantity INTEGER NOT NULL DEFAULT 1,
    item_role TEXT NOT NULL DEFAULT 'included',
    is_commissionable BOOLEAN NOT NULL DEFAULT TRUE,
    commissionable_volume INTEGER NOT NULL DEFAULT 0,
    points_value INTEGER NOT NULL DEFAULT 0
);
