-- Make messages.content optional

ALTER TABLE messages ALTER COLUMN content DROP NOT NULL;

