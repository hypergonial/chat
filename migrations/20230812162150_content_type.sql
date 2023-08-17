-- Add 'content_type' column to attachments

ALTER TABLE attachments ADD COLUMN content_type VARCHAR(255) NOT NULL DEFAULT 'application/octet-stream';
