use chrono::Utc;

use crate::models::{
    channel::{Channel, ChannelRecord, TextChannel},
    errors::{AppError, RESTError},
    guild::Guild,
    member::{ExtendedMemberRecord, Member},
    snowflake::Snowflake,
    user::User,
};

use super::Database;

/// Represents a guild record stored in the database.
pub struct GuildRecord {
    pub id: i64,
    pub name: String,
    pub owner_id: i64,
}

#[derive(Debug, Clone)]
pub struct GuildsHandler<'a> {
    db: &'a Database,
}

impl<'a> GuildsHandler<'a> {
    pub const fn new(db: &'a Database) -> Self {
        GuildsHandler { db }
    }

    /// Fetches a guild from the database by ID.
    ///
    /// ## Arguments
    ///
    /// * `guild` - The ID of the guild to fetch.
    pub async fn fetch_guild(&self, guild: impl Into<Snowflake<Guild>>) -> Option<Guild> {
        let id_64: i64 = guild.into().into();
        let record = sqlx::query_as!(
            GuildRecord,
            "SELECT id, name, owner_id FROM guilds WHERE id = $1",
            id_64
        )
        .fetch_optional(self.db.pool())
        .await
        .ok()??;

        Some(Guild::from_record(record))
    }

    /// Fetches all guilds from the database that a given user is a member of.
    ///
    /// ## Arguments
    ///
    /// * `user` - The ID of the user to fetch guilds for.
    ///
    /// ## Returns
    ///
    /// A vector of guilds that the user is a member of.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_guilds_for_user(&self, user: impl Into<Snowflake<User>>) -> Result<Vec<Guild>, sqlx::Error> {
        let user_id_64: i64 = user.into().into();
        let records = sqlx::query!(
            "SELECT guilds.id, guilds.name, guilds.owner_id 
            FROM guilds JOIN members ON guilds.id = members.guild_id 
            WHERE members.user_id = $1",
            user_id_64
        )
        .fetch_all(self.db.pool())
        .await?;

        Ok(records
            .into_iter()
            .map(|record| {
                Guild::new(
                    Snowflake::from(record.id),
                    record.name,
                    Snowflake::from(record.owner_id),
                )
            })
            .collect())
    }

    /// Fetch the owner of the guild.
    pub async fn fetch_guild_owner(&self, guild: &Guild) -> Member {
        self.fetch_member(guild.owner_id(), guild)
            .await
            .expect("Owner doesn't exist for guild, this should be impossible")
    }

    /// Fetch all members that are in the guild.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_members_for(&self, guild: impl Into<Snowflake<Guild>>) -> Result<Vec<Member>, sqlx::Error> {
        let guild_id_64: i64 = guild.into().into();

        let records = sqlx::query_as!(
            ExtendedMemberRecord,
            "SELECT members.*, users.username, users.display_name, users.last_presence 
            FROM members
            INNER JOIN users ON users.id = members.user_id
            WHERE members.guild_id = $1",
            guild_id_64
        )
        .fetch_all(self.db.pool())
        .await?;

        Ok(records.into_iter().map(Member::from_extended_record).collect())
    }

    /// Fetch all channels that are in the guild.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_channels_for(&self, guild: impl Into<Snowflake<Guild>>) -> Result<Vec<Channel>, sqlx::Error> {
        let guild_id_64: i64 = guild.into().into();

        let records = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE guild_id = $1", guild_id_64)
            .fetch_all(self.db.pool())
            .await?;

        Ok(records.into_iter().map(Channel::from_record).collect())
    }

    /// Adds a member to the guild. If the member already exists, does nothing.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn create_member(
        &self,
        guild: impl Into<Snowflake<Guild>>,
        user: impl Into<Snowflake<User>>,
    ) -> Result<(), sqlx::Error> {
        let user_id_64: i64 = user.into().into();
        let guild_id_64: i64 = guild.into().into();

        sqlx::query_as!(
            MemberRecord,
            "INSERT INTO members (user_id, guild_id, joined_at)
            VALUES ($1, $2, $3)",
            user_id_64,
            guild_id_64,
            Utc::now().timestamp(),
        )
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Removes a member from a guild.
    ///
    /// ## Errors
    ///
    /// * [`RESTError::App`] - If the database query fails.
    /// * [`RESTError::Forbidden`] - If the member is the owner of the guild.
    ///
    /// Note: If the member is the owner of the guild, this will fail.
    pub async fn delete_member(&self, guild: &Guild, user: impl Into<Snowflake<User>>) -> Result<(), RESTError> {
        let user_id = user.into();
        if guild.owner_id() == user_id {
            return Err(RESTError::Forbidden("Cannot remove owner from guild".into()));
        }

        let user_id_64: i64 = user_id.into();
        let guild_id_64: i64 = guild.id().into();
        sqlx::query!(
            "DELETE FROM members WHERE user_id = $1 AND guild_id = $2",
            user_id_64,
            guild_id_64
        )
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Fetch a member from the database by id and guild id.
    pub async fn fetch_member(
        &self,
        user: impl Into<Snowflake<User>>,
        guild: impl Into<Snowflake<Guild>>,
    ) -> Option<Member> {
        let id_64: i64 = user.into().into();
        let guild_id_64: i64 = guild.into().into();

        let record = sqlx::query_as!(
            ExtendedMemberRecord,
            "SELECT members.*, users.username, users.display_name, users.last_presence 
            FROM members
            INNER JOIN users ON users.id = members.user_id
            WHERE members.user_id = $1 AND members.guild_id = $2",
            id_64,
            guild_id_64
        )
        .fetch_optional(self.db.pool())
        .await
        .ok()??;

        Some(Member::from_extended_record(record))
    }

    /// Commit the member to the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn update_member(&self, member: &Member) -> Result<(), sqlx::Error> {
        let id_64: i64 = member.user().id().into();
        let guild_id_64: i64 = member.guild_id().into();
        sqlx::query!(
            "INSERT INTO members (user_id, guild_id, nickname, joined_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, guild_id) DO UPDATE
            SET nickname = $3, joined_at = $4",
            id_64,
            guild_id_64,
            member.nickname().as_ref(),
            member.joined_at()
        )
        .execute(self.db.pool())
        .await?;

        self.db.users().update_user(member.user()).await?;

        Ok(())
    }

    /// Create a new guild
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    ///
    /// ## Returns
    ///
    /// * [`TextChannel`] - The general text channel for the guild.
    ///
    /// Note: This will also create a general text channel for the guild.
    pub async fn create_guild(&self, guild: &Guild) -> Result<Channel, sqlx::Error> {
        let id_64: i64 = guild.id().into();
        let owner_id_i64: i64 = guild.owner_id().into();
        sqlx::query!(
            "INSERT INTO guilds (id, name, owner_id)
            VALUES ($1, $2, $3)",
            id_64,
            guild.name(),
            owner_id_i64
        )
        .execute(self.db.pool())
        .await?;

        self.create_member(guild, guild.owner_id()).await?;

        let general = TextChannel::new(guild.id().cast(), guild, "general".to_string()).into();
        self.db.channels().create_channel(&general).await?;
        Ok(general)
    }

    /// Commits the current state of this guild object to the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn update_guild(&self, guild: &Guild) -> Result<(), sqlx::Error> {
        let id_64: i64 = guild.id().into();
        let owner_id_i64: i64 = guild.owner_id().into();
        sqlx::query!(
            "INSERT INTO guilds (id, name, owner_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET name = $2, owner_id = $3",
            id_64,
            guild.name(),
            owner_id_i64
        )
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Deletes the guild.
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to delete all attachments fails.
    /// * [`AppError::Database`] - If the database query fails.
    pub async fn delete_guild(&mut self, guild: impl Into<Snowflake<Guild>>) -> Result<(), AppError> {
        let guild_id: Snowflake<Guild> = guild.into();

        self.db.app().buckets.remove_all_for_guild(guild_id).await?;

        let id_i64: i64 = guild_id.into();

        sqlx::query!("DELETE FROM guilds WHERE id = $1", id_i64)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}
