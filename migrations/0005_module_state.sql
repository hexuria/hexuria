-- Per-(module, aggregate) state blobs.
--
-- Each pay plan module persists its state as opaque JSON keyed by
-- (module_key, module_version, aggregate_id). The default aggregate is the
-- enrollment_id. The engine reads state at the start of a cascade and writes
-- any changes back inside the same transaction as the purchase writes.

CREATE TABLE module_state (
    module_key      TEXT        NOT NULL,
    module_version  TEXT        NOT NULL,
    aggregate_id    UUID        NOT NULL,
    state           JSONB       NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (module_key, module_version, aggregate_id)
);

CREATE INDEX module_state_aggregate_idx ON module_state(aggregate_id);
