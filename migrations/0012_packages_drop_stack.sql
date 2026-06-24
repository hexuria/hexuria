ALTER TABLE packages DROP COLUMN IF EXISTS pay_plan_stack_id;
ALTER TABLE package_items DROP COLUMN IF EXISTS points_value;
ALTER TABLE package_items DROP COLUMN IF EXISTS commissionable_volume;
