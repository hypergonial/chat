-- Add avatar hash to guilds
ALTER TABLE guilds
ADD COLUMN avatar_hash TEXT;