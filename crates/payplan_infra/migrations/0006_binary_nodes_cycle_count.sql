-- Track B2: add cycle_count column to binary_nodes so the event projector
-- can increment it on each BinaryCycleClosed event.
ALTER TABLE binary_nodes ADD COLUMN cycle_count INTEGER NOT NULL DEFAULT 0;
