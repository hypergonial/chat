use crate::models::{
    db::guilds::GuildRecord,
    guild::Guild,
    snowflake::Snowflake,
    user::{Presence, User, UserRecord},
};

use super::Database;

#[derive(Debug, Clone)]
pub struct UsersHandler<'a> {
    db: &'a Database,
}

impl<'a> UsersHandler<'a> {
    pub const fn new(db: &'a Database) -> Self {
        UsersHandler { db }
    }

    /// Retrieve a user from the database by their ID.
    ///
    /// ## Arguments
    ///
    /// * `user` - The ID of the user to retrieve.
    ///
    /// ## Returns
    ///
    /// The user if found, otherwise `None`.
    pub async fn fetch_user(&self, user: impl Into<Snowflake<User>>) -> Option<User> {
        let id_i64: i64 = user.into().into();
        let row = sqlx::query_as!(
            UserRecord,
            "SELECT id, username, display_name, last_presence
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(self.db.pool())
        .await
        .ok()??;

        Some(User::from_record(row))
    }

    /// Fetch the presence of a user.
    ///
    /// ## Arguments
    ///
    /// * `user` - The ID of the user to retrieve the presence of.
    ///
    /// ## Returns
    ///
    /// The presence of the user if found, otherwise `None`.
    pub async fn fetch_presence(&self, user: impl Into<Snowflake<User>>) -> Option<Presence> {
        let id_i64: i64 = user.into().into();
        let row = sqlx::query!(
            "SELECT last_presence
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(self.db.pool())
        .await
        .ok()??;

        Some(Presence::from(row.last_presence))
    }

    /// Retrieve a user from the database by their username.
    ///
    /// ## Arguments
    ///
    /// * `username` - The username of the user to retrieve.
    ///
    /// ## Returns
    ///
    /// The user if found, otherwise `None`.
    pub async fn fetch_user_by_username(&self, username: &str) -> Option<User> {
        let row = sqlx::query_as!(
            UserRecord,
            "SELECT id, username, display_name, last_presence
            FROM users
            WHERE username = $1
            LIMIT 1",
            username
        )
        .fetch_optional(self.db.pool())
        .await
        .ok()??;

        Some(User::from_record(row))
    }

    /// Fetch all guilds that this user is a member of.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_guilds_for(&self, user: impl Into<Snowflake<User>>) -> Result<Vec<Guild>, sqlx::Error> {
        let id_i64: i64 = user.into().into();

        let records = sqlx::query_as!(
            GuildRecord,
            "SELECT guilds.id, guilds.name, guilds.owner_id
            FROM guilds
            INNER JOIN members ON members.guild_id = guilds.id
            WHERE members.user_id = $1",
            id_i64
        )
        .fetch_all(self.db.pool())
        .await?;

        Ok(records.into_iter().map(Guild::from_record).collect())
    }

    /// Create a new user in the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn create_user(&self, user: &User) -> Result<(), sqlx::Error> {
        let id_i64: i64 = user.id().into();
        sqlx::query!(
            "INSERT INTO users (id, username, display_name, last_presence)
            VALUES ($1, $2, $3, $4)",
            id_i64,
            user.username(),
            user.display_name(),
            *user.last_presence() as i16
        )
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Commit this user to the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn update_user(&self, user: &User) -> Result<(), sqlx::Error> {
        let id_i64: i64 = user.id().into();
        sqlx::query!(
            "UPDATE users SET username = $2, display_name = $3, last_presence = $4
            WHERE id = $1",
            id_i64,
            user.username(),
            user.display_name(),
            *user.last_presence() as i16
        )
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
