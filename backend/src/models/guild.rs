use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::models::channel::ChannelRecord;

use super::{appstate::APP, channel::Channel, member::Member, rest::CreateGuild, snowflake::Snowflake, user::User};

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
    pub fn new(id: Snowflake, name: String, owner_id: Snowflake) -> Self {
        Self { id, name, owner_id }
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

    /// The guild's name.
    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
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
    pub async fn from_payload(payload: CreateGuild, owner_id: Snowflake) -> Self {
        Self::new(Snowflake::gen_new().await, payload.name, owner_id)
    }

    /// Fetches a guild from the database by ID.
    pub async fn fetch(id: Snowflake) -> Option<Self> {
        let db = &APP.db.read().await;
        let id_64: i64 = id.into();
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
    pub async fn fetch_all_for_user(user_id: Snowflake) -> Result<Vec<Self>, sqlx::Error> {
        let db = &APP.db.read().await;
        let user_id_64: i64 = user_id.into();
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
    pub async fn fetch_owner(&self) -> Member {
        Member::fetch(self.owner_id, self.id)
            .await
            .expect("Owner doesn't exist for guild, this should be impossible")
    }

    /// Fetch all members that are in the guild.
    pub async fn fetch_members(&self) -> Result<Vec<Member>, sqlx::Error> {
        let db = &APP.db.read().await;
        let guild_id_64: i64 = self.id.into();

        let records = sqlx::query!(
            "SELECT * FROM members JOIN users ON members.user_id = users.id WHERE members.guild_id = $1",
            guild_id_64
        )
        .fetch_all(db.pool())
        .await?;

        Ok(records
            .into_iter()
            .map(|record| {
                Member::new(
                    User::builder()
                        .username(record.username)
                        .display_name(record.display_name)
                        .id(record.user_id)
                        .build()
                        .expect("Failed building user object."),
                    Snowflake::from(record.guild_id),
                    record.nickname,
                    record.joined_at,
                )
            })
            .collect())
    }

    /// Fetch all channels that are in the guild.
    pub async fn fetch_channels(&self) -> Result<Vec<Channel>, sqlx::Error> {
        let db = &APP.db.read().await;
        let guild_id_64: i64 = self.id.into();

        let records = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE guild_id = $1", guild_id_64)
            .fetch_all(db.pool())
            .await?;

        Ok(records.into_iter().map(Channel::from_record).collect())
    }

    /// Adds a member to the guild.
    ///
    /// Note: This is faster than creating a member and then committing it.
    pub async fn create_member(&self, user_id: Snowflake) -> Result<(), sqlx::Error> {
        let db = &APP.db.read().await;
        let user_id_64: i64 = user_id.into();
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
    /// Note: If the member is the owner of the guild, this will fail.
    pub async fn remove_member(&self, user_id: Snowflake) -> Result<(), anyhow::Error> {
        if self.owner_id == user_id {
            anyhow::bail!("Cannot remove owner from guild");
        }

        let db = &APP.db.read().await;
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

    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = &APP.db.read().await;
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
}
