-- Add support for presences

ALTER TABLE "users" ADD COLUMN "last_presence" SMALLINT NOT NULL DEFAULT 0;
