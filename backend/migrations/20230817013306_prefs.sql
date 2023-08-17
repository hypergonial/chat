-- Add table for prefs

CREATE TABLE prefs (
    user_id BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    flags BIGINT NOT NULL DEFAULT 0,
    message_grouping_timeout INTEGER NOT NULL DEFAULT 60,
    layout SMALLINT NOT NULL DEFAULT 1,
    text_size SMALLINT NOT NULL DEFAULT 12,
    locale VARCHAR(5) NOT NULL DEFAULT 'en_US'
);
