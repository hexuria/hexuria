CREATE TABLE product_payplan_allocations (
    id UUID PRIMARY KEY,
    catalog_item_id UUID NOT NULL REFERENCES catalog_items(id) ON DELETE CASCADE,
    pay_plan_stack_id UUID NOT NULL REFERENCES pay_plan_stacks(id),
    points BIGINT NOT NULL DEFAULT 0 CHECK (points >= 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(catalog_item_id, pay_plan_stack_id)
);
CREATE INDEX ppa_catalog_idx ON product_payplan_allocations(catalog_item_id);
CREATE INDEX ppa_stack_idx   ON product_payplan_allocations(pay_plan_stack_id);
