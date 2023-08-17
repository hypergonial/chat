use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::appstate::APP;

use super::{snowflake::Snowflake, user::User};

/// Represents a guild member record stored in the database.
pub struct MemberRecord {
    pub user_id: i64,
    pub guild_id: i64,
    pub nickname: Option<String>,
    pub joined_at: i64,
}

/// Represents a guild member record with associated user data as queried.
pub struct ExtendedMemberRecord {
    pub user_id: i64,
    pub guild_id: i64,
    pub nickname: Option<String>,
    pub joined_at: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub last_presence: i16,
}

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
    pub fn new(user: User, guild: impl Into<Snowflake>, nickname: Option<String>, joined_at: i64) -> Self {
        let mut hasher = DefaultHasher::new();
        user.hash(&mut hasher);
        let _user_hash = hasher.finish();
        Member {
            user,
            guild_id: guild.into(),
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

    /// Build a member object directly from a database record.
    /// The user part of the object will be fetched from the database.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    pub async fn from_record(record: MemberRecord) -> Self {
        Self::new(
            User::fetch(record.user_id).await.unwrap(),
            record.guild_id,
            record.nickname,
            record.joined_at,
        )
    }

    /// Build a member object directly from a database record.
    /// The user is contained in the record, so it will not be fetched from the database.
    pub fn from_extended_record(record: ExtendedMemberRecord) -> Self {
        let mut builder = User::builder();

        if let Some(display_name) = record.display_name {
            builder.display_name(display_name);
        }

        let user = builder
            .id(record.user_id)
            .username(record.username)
            .last_presence(record.last_presence)
            .build()
            .expect("Failed to build user object.");

        Self::new(user, record.guild_id, record.nickname, record.joined_at)
    }

    /// Convert a user into a member with the given guild id.
    /// The join date of the member will be set to the current time.
    pub async fn from_user(user: User, guild: impl Into<Snowflake>) -> Self {
        Self::new(user, guild.into(), None, Utc::now().timestamp())
    }

    /// Include the user's presence field in the member payload.
    pub async fn include_presence(self) -> Self {
        let user = self.user.include_presence().await;
        Self { user, ..self }
    }

    /// Fetch a member from the database by id and guild id.
    pub async fn fetch(user: impl Into<Snowflake>, guild: impl Into<Snowflake>) -> Option<Self> {
        let db = APP.db.read().await;
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
        .fetch_optional(db.pool())
        .await
        .ok()??;

        Some(Self::from_extended_record(record))
    }

    /// Commit the member to the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = APP.db.read().await;
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

impl From<UserLike> for Snowflake {
    fn from(user_like: UserLike) -> Self {
        user_like.id()
    }
}

impl From<Member> for Snowflake {
    fn from(member: Member) -> Self {
        member.user.id()
    }
}

impl From<&UserLike> for Snowflake {
    fn from(user_like: &UserLike) -> Self {
        user_like.id()
    }
}

impl From<&Member> for Snowflake {
    fn from(member: &Member) -> Self {
        member.user.id()
    }
}
