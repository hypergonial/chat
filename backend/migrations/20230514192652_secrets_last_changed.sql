-- Add migration script here
ALTER TABLE secrets ADD COLUMN last_changed BIGINT NOT NULL DEFAULT 0;
