use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use super::snowflake::Snowflake;
use super::{guild::Guild, requests::CreateChannel, state::Config};

#[enum_dispatch(Channel)]
pub trait ChannelLike {
    /// The Snowflake ID of a channel.
    fn id(&self) -> Snowflake<Channel>;
    /// The Snowflake ID of the guild this channel belongs to.
    fn guild_id(&self) -> Snowflake<Guild>;
    /// The name of the channel.
    fn name(&self) -> &str;
    /// The name of the channel.
    fn name_mut(&mut self) -> &mut String;
    /// The type of channel.
    fn channel_type(&self) -> &'static str;
}

/// Represents a row representing a channel.
pub struct ChannelRecord {
    pub id: Snowflake<Channel>,
    pub guild_id: Snowflake<Guild>,
    pub name: String,
    pub channel_type: String,
}

#[non_exhaustive]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[enum_dispatch]
pub enum Channel {
    GuildText(TextChannel),
}

impl Channel {
    pub fn from_record(record: ChannelRecord) -> Self {
        match record.channel_type.as_str() {
            "TEXT_CHANNEL" => Self::GuildText(TextChannel::new(record.id, record.guild_id, record.name)),
            _ => panic!("Invalid channel type"),
        }
    }

    pub fn from_payload(config: &Config, payload: CreateChannel, guild_id: Snowflake<Guild>) -> Self {
        match payload {
            CreateChannel::GuildText { name } => {
                Self::GuildText(TextChannel::new(Snowflake::gen_new(config), guild_id, name))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TextChannel {
    id: Snowflake<Channel>,
    guild_id: Snowflake<Guild>,
    name: String,
}

impl TextChannel {
    pub fn new(id: Snowflake<Channel>, guild: impl Into<Snowflake<Guild>>, name: String) -> Self {
        Self {
            id,
            guild_id: guild.into(),
            name,
        }
    }
}

impl ChannelLike for TextChannel {
    fn id(&self) -> Snowflake<Channel> {
        self.id
    }

    fn guild_id(&self) -> Snowflake<Guild> {
        self.guild_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn channel_type(&self) -> &'static str {
        "TEXT_CHANNEL"
    }
}

impl From<Channel> for Snowflake<Channel> {
    fn from(channel: Channel) -> Self {
        channel.id()
    }
}

impl From<TextChannel> for Snowflake<Channel> {
    fn from(channel: TextChannel) -> Self {
        channel.id()
    }
}

impl From<&Channel> for Snowflake<Channel> {
    fn from(channel: &Channel) -> Self {
        channel.id()
    }
}

impl From<&TextChannel> for Snowflake<Channel> {
    fn from(channel: &TextChannel) -> Self {
        channel.id()
    }
}

impl From<&mut Channel> for Snowflake<Channel> {
    fn from(channel: &mut Channel) -> Self {
        channel.id()
    }
}

impl From<&mut TextChannel> for Snowflake<Channel> {
    fn from(channel: &mut TextChannel) -> Self {
        channel.id()
    }
}
