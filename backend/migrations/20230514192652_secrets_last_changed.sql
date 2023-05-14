-- Add last_changed column to secrets table to track password changes.
ALTER TABLE secrets ADD COLUMN last_changed BIGINT NOT NULL DEFAULT 0;
