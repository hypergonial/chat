-- Add attachment support

CREATE TABLE "attachments" (
    "id" INT NOT NULL,
    "message_id" BIGINT NOT NULL REFERENCES "messages" ("id") ON DELETE CASCADE,
    "filename" TEXT NOT NULL,
    PRIMARY KEY ("id", "message_id")
)
