use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::appstate::APP;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{snowflake::Snowflake, user::User};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Member {
    /// The user this guild member represents
    user: User,
    /// The id of the guild this member is in
    guild_id: Snowflake,
    /// Nickname of the user in this guild, if set
    nickname: Option<String>,
    /// UNIX timestmap of when the user joined the guild
    joined_at: i64,
    /// Used to check if the user was mutated and needs re-committing when calling commit()
    #[serde(skip)]
    _user_hash: u64,
}

impl Member {
    /// Create a new member with the given user, guild id, nickname, and joined at timestamp.
    pub fn new(user: User, guild_id: Snowflake, nickname: Option<String>, joined_at: i64) -> Self {
        let mut hasher = DefaultHasher::new();
        user.hash(&mut hasher);
        let _user_hash = hasher.finish();
        Member {
            user,
            guild_id,
            nickname,
            joined_at,
            _user_hash,
        }
    }

    /// The user this guild member represents
    pub fn user(&self) -> &User {
        &self.user
    }

    /// The id of the guild this member is in
    pub fn guild_id(&self) -> Snowflake {
        self.guild_id
    }

    /// Nickname of the user in this guild, if set
    pub fn nickname(&self) -> &Option<String> {
        &self.nickname
    }

    /// UNIX timestmap of when the user joined the guild
    pub fn joined_at(&self) -> i64 {
        self.joined_at
    }

    /// Mutable handle to the user this guild member represents
    pub fn user_mut(&mut self) -> &mut User {
        &mut self.user
    }

    /// Convert a user into a member with the given guild id.
    pub async fn from_user(user: User, guild_id: Snowflake) -> Self {
        Self::new(user, guild_id, None, Utc::now().timestamp())
    }

    /// Fetch a member from the database by id and guild id.
    pub async fn fetch(id: Snowflake, guild_id: Snowflake) -> Option<Self> {
        let db = &APP.read().await.db;
        let id_64: i64 = id.into();
        let guild_id_64: i64 = guild_id.into();

        let record = sqlx::query!(
            "SELECT * FROM members WHERE user_id = $1 AND guild_id = $2",
            id_64,
            guild_id_64
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;

        Some(Self::new(
            User::fetch(id).await?,
            guild_id,
            record.nickname,
            record.joined_at,
        ))
    }

    /// Commit the member to the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = &APP.read().await.db;
        let id_64: i64 = self.user.id().into();
        let guild_id_64: i64 = self.guild_id.into();
        sqlx::query!(
            "INSERT INTO members (user_id, guild_id, nickname, joined_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, guild_id) DO UPDATE
            SET nickname = $3, joined_at = $4",
            id_64,
            guild_id_64,
            self.nickname,
            self.joined_at
        )
        .execute(db.pool())
        .await?;

        let mut hasher = DefaultHasher::new();
        self.user.hash(&mut hasher);
        let _current_hash = hasher.finish();

        if _current_hash != self._user_hash {
            self.user.commit().await?;
        }

        Ok(())
    }
}

/// A user or member, depending on the context.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum UserLike {
    Member(Member),
    User(User),
}

impl UserLike {
    pub fn id(&self) -> Snowflake {
        match self {
            UserLike::Member(member) => member.user.id(),
            UserLike::User(user) => user.id(),
        }
    }
}
