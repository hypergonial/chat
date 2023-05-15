-- Add last_changed field to secrets
ALTER TABLE secrets ADD COLUMN last_changed BIGINT NOT NULL DEFAULT 0;
