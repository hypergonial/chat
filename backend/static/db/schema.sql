DO $$
BEGIN
    CREATE OR REPLACE FUNCTION createSchema() RETURNS void AS
    $BODY$
    BEGIN
        CREATE TABLE IF NOT EXISTS schema_info
        (
            schema_version INTEGER NOT NULL,
            PRIMARY KEY (schema_version)
        );

        -- Insert schema version into schema_info table if not already present
        DO
        $do$
        DECLARE _schema_version integer;
        BEGIN
            SELECT 1 INTO _schema_version; -- The current schema version, change this when creating new migrations

            IF NOT EXISTS (SELECT schema_version FROM schema_info) THEN
                INSERT INTO schema_info (schema_version) 
                VALUES (_schema_version); 
            END IF;
        END
        $do$;

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
            PRIMARY KEY (user_id),
            FOREIGN KEY (user_id) 
                REFERENCES users(id)
                ON DELETE CASCADE
        );
    END;
    $BODY$
    LANGUAGE plpgsql;
END; $$
