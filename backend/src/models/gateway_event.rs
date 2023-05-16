use super::{
    member::{Member, UserLike},
    message::Message,
    snowflake::Snowflake,
    user::User,
};
use secrecy::Secret;
use serde::{Deserialize, Serialize};

/// A JSON payload that can be received over the websocket by clients.
/// All events are serialized in a way such that they are wrapped in a "data" field.
#[derive(Serialize, Debug, Clone)]
#[non_exhaustive]
#[serde(tag = "event", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GatewayEvent {
    /// A chat message.
    MessageCreate(Message),
    /// A peer has joined the chat.
    MemberCreate(Member),
    /// A peer has left the chat.
    MemberRemove(Member),
    /// The server is ready to accept messages.
    Ready(User),
    /// The server has closed the connection.
    InvalidSession(String),
}

impl EventLike for GatewayEvent {
    fn extract_guild_id(&self) -> Option<Snowflake> {
        match self {
            Self::MessageCreate(message) => message.extract_guild_id(),
            Self::MemberCreate(member) => member.extract_guild_id(),
            Self::MemberRemove(member) => member.extract_guild_id(),
            Self::Ready(_) => None,
            Self::InvalidSession(_) => None,
        }
    }
}

pub trait EventLike {
    fn extract_guild_id(&self) -> Option<Snowflake>;
}

impl EventLike for Message {
    fn extract_guild_id(&self) -> Option<Snowflake> {
        if let Some(UserLike::Member(member)) = &self.author() {
            Some(member.guild_id())
        } else {
            None
        }
    }
}

impl EventLike for Member {
    fn extract_guild_id(&self) -> Option<Snowflake> {
        Some(self.guild_id())
    }
}

/// A JSON payload that can be sent over the websocket by clients.
#[derive(Deserialize, Debug, Clone)]
#[non_exhaustive]
#[serde(tag = "event", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GatewayMessage {
    /// Identify with the server. This should be the first event sent by the client.
    Identify(IdentifyPayload),
}

#[derive(Deserialize, Debug, Clone)]
pub struct IdentifyPayload {
    pub token: Secret<String>,
}
