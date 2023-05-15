-- Initial schema

CREATE TABLE IF NOT EXISTS "users"
(
    "id" BIGINT PRIMARY KEY,
    "username" TEXT NOT NULL UNIQUE,
    "display_name" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "secrets"
(
    "user_id" BIGINT PRIMARY KEY REFERENCES "users"("id") ON DELETE CASCADE,
    "password" TEXT NOT NULL,
    "is_valid" BOOLEAN NOT NULL DEFAULT TRUE
);
