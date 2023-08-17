-- Make display_name NULL-able

ALTER TABLE users ALTER COLUMN display_name DROP NOT NULL;