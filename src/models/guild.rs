use serde::{Deserialize, Serialize};

use super::{appstate::Config, db::guilds::GuildRecord, requests::CreateGuild, snowflake::Snowflake, user::User};

/// Represents a guild.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Guild {
    id: Snowflake<Guild>,
    name: String,
    owner_id: Snowflake<User>,
}

impl Guild {
    /// Create a new guild with the given id, name, and owner id.
    ///
    /// ## Arguments
    ///
    /// * `id` - The guild's ID.
    /// * `name` - The guild's name.
    /// * `owner` - The guild's owner.
    pub fn new(id: Snowflake<Self>, name: String, owner: impl Into<Snowflake<User>>) -> Self {
        Self {
            id,
            name,
            owner_id: owner.into(),
        }
    }

    /// The guild's ID.
    pub const fn id(&self) -> Snowflake<Self> {
        self.id
    }

    /// The guild's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The guild's owner's ID.
    pub const fn owner_id(&self) -> Snowflake<User> {
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
    pub fn from_payload(config: &Config, payload: CreateGuild, owner: impl Into<Snowflake<User>>) -> Self {
        Self::new(Snowflake::gen_new(config), payload.name, owner.into())
    }
}

impl From<Guild> for Snowflake<Guild> {
    fn from(guild: Guild) -> Self {
        guild.id()
    }
}

impl From<&Guild> for Snowflake<Guild> {
    fn from(guild: &Guild) -> Self {
        guild.id()
    }
}

impl From<&mut Guild> for Snowflake<Guild> {
    fn from(guild: &mut Guild) -> Self {
        guild.id()
    }
}
