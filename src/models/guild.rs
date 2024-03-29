use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::models::{channel::ChannelRecord, member::ExtendedMemberRecord};

use super::{
    appstate::APP,
    channel::Channel,
    errors::{AppError, RESTError},
    member::Member,
    requests::CreateGuild,
    snowflake::Snowflake,
};

/// Represents a guild record stored in the database.
pub struct GuildRecord {
    pub id: i64,
    pub name: String,
    pub owner_id: i64,
}

/// Represents a guild.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Guild {
    id: Snowflake,
    name: String,
    owner_id: Snowflake,
}

impl Guild {
    /// Create a new guild with the given id, name, and owner id.
    ///
    /// ## Arguments
    ///
    /// * `id` - The guild's ID.
    /// * `name` - The guild's name.
    /// * `owner` - The guild's owner.
    pub fn new(id: Snowflake, name: String, owner: impl Into<Snowflake>) -> Self {
        Self {
            id,
            name,
            owner_id: owner.into(),
        }
    }

    /// The guild's ID.
    pub fn id(&self) -> Snowflake {
        self.id
    }

    /// The guild's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The guild's owner's ID.
    pub fn owner_id(&self) -> Snowflake {
        self.owner_id
    }

    /// Create a new guild object from a database record.
    pub fn from_record(record: GuildRecord) -> Self {
        Self {
            id: record.id.into(),
            name: record.name,
            owner_id: record.owner_id.into(),
        }
    }

    /// Constructs a new guild from a payload and owner ID.
    ///
    /// ## Arguments
    ///
    /// * `payload` - The payload to construct the guild from.
    /// * `owner` - The ID of the guild's owner.
    pub async fn from_payload(payload: CreateGuild, owner: impl Into<Snowflake>) -> Self {
        Self::new(Snowflake::gen_new(), payload.name, owner.into())
    }

    /// Fetches a guild from the database by ID.
    ///
    /// ## Arguments
    ///
    /// * `guild` - The ID of the guild to fetch.
    pub async fn fetch(guild: impl Into<Snowflake>) -> Option<Self> {
        let db = APP.db.read().await;
        let id_64: i64 = guild.into().into();
        let record = sqlx::query_as!(
            GuildRecord,
            "SELECT id, name, owner_id FROM guilds WHERE id = $1",
            id_64
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;

        Some(Self::from_record(record))
    }

    /// Fetches all guilds from the database that a given user is a member of.
    ///
    /// ## Arguments
    ///
    /// * `user` - The ID of the user to fetch guilds for.
    pub async fn fetch_all_for_user(user: impl Into<Snowflake>) -> Result<Vec<Self>, sqlx::Error> {
        let db = APP.db.read().await;
        let user_id_64: i64 = user.into().into();
        let records = sqlx::query!(
            "SELECT guilds.id, guilds.name, guilds.owner_id 
            FROM guilds JOIN members ON guilds.id = members.guild_id 
            WHERE members.user_id = $1",
            user_id_64
        )
        .fetch_all(db.pool())
        .await?;

        Ok(records
            .into_iter()
            .map(|record| {
                Self::new(
                    Snowflake::from(record.id),
                    record.name,
                    Snowflake::from(record.owner_id),
                )
            })
            .collect())
    }

    /// Fetch the owner of the guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    pub async fn fetch_owner(&self) -> Member {
        Member::fetch(self.owner_id, self.id)
            .await
            .expect("Owner doesn't exist for guild, this should be impossible")
    }

    /// Fetch all members that are in the guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_members(&self) -> Result<Vec<Member>, sqlx::Error> {
        let db = APP.db.read().await;
        let guild_id_64: i64 = self.id.into();

        let records = sqlx::query_as!(
            ExtendedMemberRecord,
            "SELECT members.*, users.username, users.display_name, users.last_presence 
            FROM members
            INNER JOIN users ON users.id = members.user_id
            WHERE members.guild_id = $1",
            guild_id_64
        )
        .fetch_all(db.pool())
        .await?;

        Ok(records.into_iter().map(Member::from_extended_record).collect())
    }

    /// Fetch all channels that are in the guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_channels(&self) -> Result<Vec<Channel>, sqlx::Error> {
        let db = APP.db.read().await;
        let guild_id_64: i64 = self.id.into();

        let records = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE guild_id = $1", guild_id_64)
            .fetch_all(db.pool())
            .await?;

        Ok(records.into_iter().map(Channel::from_record).collect())
    }

    /// Adds a member to the guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    ///
    /// Note: This is faster than creating a member and then committing it.
    pub async fn create_member(&self, user: impl Into<Snowflake>) -> Result<(), sqlx::Error> {
        let db = APP.db.read().await;
        let user_id_64: i64 = user.into().into();
        let guild_id_64: i64 = self.id.into();
        sqlx::query!(
            "INSERT INTO members (user_id, guild_id, joined_at)
            VALUES ($1, $2, $3) ON CONFLICT (user_id, guild_id) DO NOTHING",
            user_id_64,
            guild_id_64,
            Utc::now().timestamp(),
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }

    /// Removes a member from a guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`RESTError::App`] - If the database query fails.
    /// * [`RESTError::Forbidden`] - If the member is the owner of the guild.
    ///
    /// Note: If the member is the owner of the guild, this will fail.
    pub async fn remove_member(&self, user: impl Into<Snowflake>) -> Result<(), RESTError> {
        let user_id = user.into();
        if self.owner_id == user_id {
            return Err(RESTError::Forbidden("Cannot remove owner from guild".into()));
        }

        let db = APP.db.read().await;
        let user_id_64: i64 = user_id.into();
        let guild_id_64: i64 = self.id.into();
        sqlx::query!(
            "DELETE FROM members WHERE user_id = $1 AND guild_id = $2",
            user_id_64,
            guild_id_64
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }

    /// Commits the current state of this guild object to the database.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = APP.db.read().await;
        let id_64: i64 = self.id.into();
        let owner_id_i64: i64 = self.owner_id.into();
        sqlx::query!(
            "INSERT INTO guilds (id, name, owner_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET name = $2, owner_id = $3",
            id_64,
            self.name,
            owner_id_i64
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }

    /// Deletes the guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to delete all attachments fails.
    /// * [`AppError::Database`] - If the database query fails.
    pub async fn delete(self) -> Result<(), AppError> {
        let db = APP.db.read().await;
        let id_64: i64 = self.id.into();

        APP.buckets().remove_all_for_guild(self).await?;

        sqlx::query!("DELETE FROM guilds WHERE id = $1", id_64)
            .execute(db.pool())
            .await?;
        Ok(())
    }
}

impl From<Guild> for Snowflake {
    fn from(guild: Guild) -> Self {
        guild.id()
    }
}

impl From<&Guild> for Snowflake {
    fn from(guild: &Guild) -> Self {
        guild.id()
    }
}
