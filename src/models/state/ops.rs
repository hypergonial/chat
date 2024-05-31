use chrono::Utc;

use crate::models::{
    attachment::{Attachment, AttachmentLike, FullAttachment},
    channel::{Channel, ChannelLike, ChannelRecord, TextChannel},
    errors::{AppError, RESTError},
    guild::{Guild, GuildRecord},
    member::{ExtendedMemberRecord, Member, MemberRecord},
    message::{ExtendedMessageRecord, Message},
    requests::{CreateGuild, CreateUser},
    snowflake::Snowflake,
    user::{Presence, User, UserRecord},
};

use super::ApplicationState;

/// Contains all the application state operations.
pub struct Ops<'a> {
    app: &'a ApplicationState,
}

impl<'a> Ops<'a> {
    /// Create a new application state operations.
    pub const fn new(app: &'a ApplicationState) -> Self {
        Self { app }
    }

    /// Fetch a channel from the database by ID.
    ///
    /// ## Arguments
    ///
    /// * `id` - The ID of the channel to fetch.
    ///
    /// ## Returns
    ///
    /// The channel if found, otherwise `None`.
    pub async fn fetch_channel(&self, id: impl Into<Snowflake<Channel>>) -> Option<Channel> {
        let id_64: i64 = id.into().into();

        let record = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE id = $1", id_64)
            .fetch_optional(self.app.db.pool())
            .await
            .ok()??;

        Some(Channel::from_record(record))
    }

    /// Create a new channel in the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn create_channel(&self, channel: &Channel) -> Result<Channel, sqlx::Error> {
        let id_64: i64 = channel.id().into();
        let guild_id_64: i64 = channel.guild_id().into();
        sqlx::query_as!(
            ChannelRecord,
            "INSERT INTO channels (id, guild_id, name, channel_type)
            VALUES ($1, $2, $3, $4) RETURNING *",
            id_64,
            guild_id_64,
            channel.name(),
            channel.channel_type(),
        )
        .fetch_one(self.app.db.pool())
        .await
        .map(Channel::from_record)
    }

    /// Commit this channel to the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn update_channel(&self, channel: &Channel) -> Result<(), sqlx::Error> {
        let id_64: i64 = channel.id().into();
        sqlx::query!("UPDATE channels SET name = $2 WHERE id = $1", id_64, channel.name(),)
            .execute(self.app.db.pool())
            .await?;

        Ok(())
    }

    /// Deletes the channel.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to delete all attachments fails.
    /// * [`AppError::Database`] - If the database query fails.
    pub async fn delete_channel(&mut self, channel: impl Into<Snowflake<Channel>>) -> Result<(), AppError> {
        let channel_id: Snowflake<Channel> = channel.into();

        self.app.s3.remove_all_for_channel(channel_id).await?;

        let id_i64: i64 = channel_id.into();

        sqlx::query!("DELETE FROM channels WHERE id = $1", id_i64)
            .execute(self.app.db.pool())
            .await?;

        Ok(())
    }

    /// Fetch messages from this channel.
    ///
    /// ## Arguments
    ///
    /// * `limit` - The maximum number of messages to fetch. Defaults to 50, capped at 100.
    /// * `before` - Fetch messages before this ID.
    /// * `after` - Fetch messages after this ID.
    ///
    /// ## Returns
    ///
    /// [`Vec<Message>`] - The messages fetched.
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_messages_from(
        &self,
        channel: impl Into<Snowflake<Channel>>,
        limit: Option<u32>,
        before: Option<Snowflake<Message>>,
        after: Option<Snowflake<Message>>,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        let id_64: i64 = channel.into().into();

        let records: Vec<ExtendedMessageRecord> = if before.is_none() && after.is_none() {
            sqlx::query_as!(
                ExtendedMessageRecord,
                "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
                FROM messages
                LEFT JOIN users ON messages.user_id = users.id
                LEFT JOIN attachments ON messages.id = attachments.message_id
                WHERE messages.channel_id = $1
                ORDER BY messages.id DESC LIMIT $2",
                id_64,
                i64::from(limit)
            )
            .fetch_all(self.app.db.pool())
            .await?
        } else {
            sqlx::query_as!(
                ExtendedMessageRecord,
                "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
                FROM messages
                LEFT JOIN users ON messages.user_id = users.id
                LEFT JOIN attachments ON messages.id = attachments.message_id
                WHERE messages.channel_id = $1 AND messages.id > $2 AND messages.id < $3
                ORDER BY messages.id DESC LIMIT $4",
                id_64,
                before.map_or(i64::MAX, Into::into),
                after.map_or(i64::MIN, Into::into),
                i64::from(limit)
            )
            .fetch_all(self.app.db.pool())
            .await?
        };
        Ok(Message::from_records(&records))
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
        .fetch_optional(self.app.db.pool())
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
        .fetch_all(self.app.db.pool())
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
        .fetch_all(self.app.db.pool())
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
            .fetch_all(self.app.db.pool())
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
    ) -> Result<Member, sqlx::Error> {
        let user_id = user.into();

        let user = self.fetch_user(user_id).await.ok_or(sqlx::Error::RowNotFound)?;

        let user_id_64: i64 = user_id.into();
        let guild_id_64: i64 = guild.into().into();

        let record = sqlx::query_as!(
            MemberRecord,
            "INSERT INTO members (user_id, guild_id, joined_at)
            VALUES ($1, $2, $3) RETURNING *",
            user_id_64,
            guild_id_64,
            Utc::now().timestamp(),
        )
        .fetch_one(self.app.db.pool())
        .await?;
        Ok(Member::from_record(user, record))
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
        .execute(self.app.db.pool())
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
        .fetch_optional(self.app.db.pool())
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
        .execute(self.app.db.pool())
        .await?;

        self.app.ops().update_user(member.user()).await?;

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
    /// * [`Guild`] - The created guild.
    /// * [`Channel`] - The general text channel for the guild.
    /// * [`Member`] - The owner of the guild.
    ///
    /// Note: This will also create a general text channel for the guild.
    pub async fn create_guild(
        &self,
        payload: CreateGuild,
        owner: impl Into<Snowflake<User>>,
    ) -> Result<(Guild, Channel, Member), sqlx::Error> {
        let guild = Guild::from_payload(&self.app.config, payload, owner);

        let id_64: i64 = guild.id().into();
        let owner_id_i64: i64 = guild.owner_id().into();
        sqlx::query!(
            "INSERT INTO guilds (id, name, owner_id)
            VALUES ($1, $2, $3)",
            id_64,
            guild.name(),
            owner_id_i64
        )
        .execute(self.app.db.pool())
        .await?;

        let member = self.create_member(&guild, guild.owner_id()).await?;

        let general = TextChannel::new(guild.id().cast(), &guild, "general".to_string()).into();
        self.app.ops().create_channel(&general).await?;
        Ok((guild, general, member))
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
        .execute(self.app.db.pool())
        .await?;
        Ok(())
    }

    /// Deletes the guild.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to delete all attachments fails.
    /// * [`AppError::Database`] - If the database query fails.
    pub async fn delete_guild(&mut self, guild: impl Into<Snowflake<Guild>>) -> Result<(), AppError> {
        let guild_id: Snowflake<Guild> = guild.into();

        self.app.s3.remove_all_for_guild(guild_id).await?;

        let id_i64: i64 = guild_id.into();

        sqlx::query!("DELETE FROM guilds WHERE id = $1", id_i64)
            .execute(self.app.db.pool())
            .await?;
        Ok(())
    }

    /// Retrieve a message and fetch its author from the database in one query.
    /// Attachment contents will not be retrieved from S3.
    ///
    /// ## Arguments
    ///
    /// * `message` - The ID of the message to retrieve.
    ///
    /// ## Returns
    ///
    /// The message if found, otherwise `None`.
    pub async fn fetch_message(&self, message: impl Into<Snowflake<Message>>) -> Option<Message> {
        let id_i64: i64 = message.into().into();

        let records = sqlx::query_as!(
            ExtendedMessageRecord,
            "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
            FROM messages
            LEFT JOIN users ON messages.user_id = users.id
            LEFT JOIN attachments ON messages.id = attachments.message_id
            WHERE messages.id = $1",
            id_i64
        )
        .fetch_all(self.app.db.pool())
        .await
        .ok()?;

        Message::from_records(&records).pop()
    }

    /// Commit this message to the database. Uploads all attachments to S3.
    /// It is highly recommended to call [`Message::strip_attachment_contents`] after calling
    /// this method to remove the attachment contents from memory.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to upload one of the attachments fails.
    /// * [`AppError::Database`] - If the database request fails.
    pub async fn update_message(&self, message: &Message) -> Result<(), AppError> {
        let id_i64: i64 = message.id().into();
        let author_id_i64: Option<i64> = message.author().map(|u| u.id().into());
        let channel_id_i64: i64 = message.channel_id().into();
        sqlx::query!(
            "INSERT INTO messages (id, user_id, channel_id, content)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET user_id = $2, channel_id = $3, content = $4",
            id_i64,
            author_id_i64,
            channel_id_i64,
            message.content(),
        )
        .execute(self.app.db.pool())
        .await?;

        for attachment in message.attachments() {
            if let Attachment::Full(f) = attachment {
                self.create_attachment(f).await?;
            }
        }
        Ok(())
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
            "SELECT id, username, display_name, avatar_hash, last_presence
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(self.app.db.pool())
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
        .fetch_optional(self.app.db.pool())
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
            "SELECT id, username, display_name, avatar_hash, last_presence
            FROM users
            WHERE username = $1
            LIMIT 1",
            username
        )
        .fetch_optional(self.app.db.pool())
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
        .fetch_all(self.app.db.pool())
        .await?;

        Ok(records.into_iter().map(Guild::from_record).collect())
    }

    /// Create a new user in the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn create_user(&self, payload: CreateUser) -> Result<User, sqlx::Error> {
        let id_i64: i64 = Snowflake::<User>::gen_new(&self.app.config).into();

        sqlx::query_as!(
            UserRecord,
            "INSERT INTO users (id, username)
            VALUES ($1, $2) RETURNING *",
            id_i64,
            payload.username,
        )
        .fetch_one(self.app.db.pool())
        .await
        .map(User::from_record)
    }

    /// Commit this user to the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    ///
    /// ## Returns
    ///
    /// The user if the commit was successful.
    pub async fn update_user(&self, user: &User) -> Result<(), sqlx::Error> {
        let id_i64: i64 = user.id().into();

        let old_user = self.fetch_user(user.id()).await.ok_or(sqlx::Error::RowNotFound)?;

        if old_user.avatar_hash() != user.avatar_hash() {
            // TODO: upload avatar to S3
            // Making an avatar type would be nice, but is actually literal hell
        }

        sqlx::query!(
            "UPDATE users SET username = $2, display_name = $3, last_presence = $4, avatar_hash = $5
            WHERE id = $1",
            id_i64,
            user.username(),
            user.display_name(),
            *user.last_presence() as i16,
            user.avatar_hash()
        )
        .execute(self.app.db.pool())
        .await?;
        Ok(())
    }

    /// Commit the attachment to the database. Uploads the contents to S3 implicitly.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn create_attachment(&self, attachment: &FullAttachment) -> Result<(), AppError> {
        let message_id: i64 = attachment.message_id().into();
        let channel_id: i64 = attachment.channel_id().into();

        attachment.upload(&self.app.s3).await?;

        sqlx::query!(
            "INSERT INTO attachments (id, filename, message_id, channel_id, content_type)
            VALUES ($1, $2, $3, $4, $5) 
            ON CONFLICT (id, message_id) 
            DO UPDATE SET filename = $2, content_type = $5",
            i32::from(attachment.id()),
            attachment.filename(),
            message_id,
            channel_id,
            attachment.mime().to_string(),
        )
        .execute(self.app.db.pool())
        .await?;

        Ok(())
    }
}
