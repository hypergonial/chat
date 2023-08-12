-- Add column 'channel_id' to 'attachments'

ALTER TABLE attachments ADD COLUMN channel_id BIGINT;

UPDATE attachments SET channel_id = messages.channel_id
FROM messages
WHERE attachments.message_id = messages.id;

ALTER TABLE attachments ALTER COLUMN channel_id SET NOT NULL;
