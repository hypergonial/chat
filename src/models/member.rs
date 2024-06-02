use chrono::Utc;
use serde::Serialize;

use crate::gateway::handler::Gateway;

use super::{
    avatar::{Avatar, PartialAvatar},
    errors::BuildError,
    guild::Guild,
};

use super::{snowflake::Snowflake, user::User};

/// Represents a guild member record stored in the database.
pub struct MemberRecord {
    pub user_id: Snowflake<User>,
    pub guild_id: Snowflake<Guild>,
    pub nickname: Option<String>,
    pub joined_at: i64,
}

/// Represents a guild member record with associated user data as queried.
pub struct ExtendedMemberRecord {
    pub user_id: Snowflake<User>,
    pub guild_id: Snowflake<Guild>,
    pub nickname: Option<String>,
    pub joined_at: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_hash: Option<String>,
    pub last_presence: i16,
}

#[derive(Serialize, Debug, Clone)]
pub struct Member {
    /// The user this guild member represents
    user: User,
    /// The id of the guild this member is in
    guild_id: Snowflake<Guild>,
    /// Nickname of the user in this guild, if set
    nickname: Option<String>,
    /// UNIX timestmap of when the user joined the guild
    joined_at: i64,
}

impl Member {
    /// Create a new member with the given user, guild id, nickname, and joined at timestamp.
    pub fn new(user: User, guild: impl Into<Snowflake<Guild>>, nickname: Option<String>, joined_at: i64) -> Self {
        Self {
            user,
            guild_id: guild.into(),
            nickname,
            joined_at,
        }
    }

    /// The user this guild member represents
    pub const fn user(&self) -> &User {
        &self.user
    }

    /// The id of the guild this member is in
    pub const fn guild_id(&self) -> Snowflake<Guild> {
        self.guild_id
    }

    /// Nickname of the user in this guild, if set
    pub const fn nickname(&self) -> &Option<String> {
        &self.nickname
    }

    /// UNIX timestmap of when the user joined the guild
    pub const fn joined_at(&self) -> i64 {
        self.joined_at
    }

    /// Mutable handle to the user this guild member represents
    pub fn user_mut(&mut self) -> &mut User {
        &mut self.user
    }

    /// Build a member object directly from a database record and a user
    pub fn from_record(user: User, record: MemberRecord) -> Self {
        Self::new(user, record.guild_id, record.nickname, record.joined_at)
    }

    /// Build a member object directly from a database record.
    /// The user is contained in the record, so it will not be fetched from the database.
    ///
    /// # Returns
    ///
    /// Returns a `Member` object if the member could be built.
    ///
    /// # Errors
    ///
    /// Returns a `BuildError` if the user object could not be built.
    pub fn from_extended_record(record: ExtendedMemberRecord) -> Result<Self, BuildError> {
        let mut builder = User::builder();

        if let Some(display_name) = record.display_name {
            builder.display_name(display_name);
        }

        if let Some(avatar_hash) = record.avatar_hash {
            builder.avatar(Avatar::Partial(PartialAvatar::new(avatar_hash, record.user_id)?));
        }

        let user = builder
            .id(record.user_id)
            .username(record.username)
            .last_presence(record.last_presence)
            .build()
            .expect("Failed to build user object.");

        Ok(Self::new(user, record.guild_id, record.nickname, record.joined_at))
    }

    /// Convert a user into a member with the given guild id.
    /// The join date of the member will be set to the current time.
    pub fn from_user(user: User, guild: impl Into<Snowflake<Guild>>) -> Self {
        Self::new(user, guild.into(), None, Utc::now().timestamp())
    }

    /// Include the user's presence field in the member payload.
    #[must_use]
    pub fn include_presence(self, gateway: &Gateway) -> Self {
        let user = self.user.include_presence(gateway);
        Self { user, ..self }
    }
}

/// A user or member, depending on the context.
#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum UserLike {
    Member(Member),
    User(User),
}

impl UserLike {
    pub const fn id(&self) -> Snowflake<User> {
        match self {
            Self::Member(member) => member.user.id(),
            Self::User(user) => user.id(),
        }
    }
}

impl From<UserLike> for Snowflake<User> {
    fn from(user_like: UserLike) -> Self {
        user_like.id()
    }
}

impl From<Member> for Snowflake<User> {
    fn from(member: Member) -> Self {
        member.user.id()
    }
}

impl From<&UserLike> for Snowflake<User> {
    fn from(user_like: &UserLike) -> Self {
        user_like.id()
    }
}

impl From<&Member> for Snowflake<User> {
    fn from(member: &Member) -> Self {
        member.user.id()
    }
}
