DO $$
BEGIN
    CREATE OR REPLACE FUNCTION createSchema() RETURNS void AS
    $BODY$
    BEGIN
        CREATE TABLE IF NOT EXISTS users
        (
            id BIGINT NOT NULL,
            username TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            PRIMARY KEY (id)
        );

        CREATE TABLE IF NOT EXISTS messages
        (
            id BIGINT NOT NULL,
            user_id BIGINT NOT NULL,
            content TEXT NOT NULL,
            PRIMARY KEY (id),
            FOREIGN KEY (user_id) 
                REFERENCES users(id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS secrets
        (
            user_id BIGINT NOT NULL,
            password TEXT NOT NULL,
            is_valid BOOLEAN NOT NULL DEFAULT TRUE,
            last_changed BIGINT NOT NULL DEFAULT 0,
            PRIMARY KEY (user_id),
            FOREIGN KEY (user_id) 
                REFERENCES users(id)
                ON DELETE CASCADE
        );
    END;
    $BODY$
    LANGUAGE plpgsql;
END; $$
