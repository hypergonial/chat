-- Add avatar hash to users
ALTER TABLE users
ADD COLUMN avatar_hash TEXT;