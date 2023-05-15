-- Add new tables to support channels and guilds

CREATE TABLE IF NOT EXISTS "guilds"
(
    "id" BIGINT PRIMARY KEY,
    "owner_id" BIGINT NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "name" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "channels"
(
    "id" BIGINT PRIMARY KEY,
    "channel_type" TEXT NOT NULL,
    "guild_id" BIGINT NOT NULL REFERENCES "guilds" ("id") ON DELETE CASCADE,
    name TEXT NOT NULL
);

DROP TABLE IF EXISTS "messages";

CREATE TABLE "messages"
(
    "id" BIGINT PRIMARY KEY,
    "user_id" BIGINT REFERENCES "users" ("id") ON DELETE SET NULL,
    "channel_id" BIGINT NOT NULL REFERENCES "channels" ("id") ON DELETE CASCADE,
    "content" TEXT NOT NULL
);


CREATE TABLE IF NOT EXISTS "members"
(
    "user_id" BIGINT NOT NULL REFERENCES "users" ("id") ON DELETE CASCADE,
    "guild_id" BIGINT NOT NULL REFERENCES "guilds" ("id") ON DELETE CASCADE,
    "nickname" TEXT,
    "joined_at" BIGINT NOT NULL,
    PRIMARY KEY ("user_id", "guild_id")
);
