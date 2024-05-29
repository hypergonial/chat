use futures::future;
use secrecy::Secret;
use serde::{Deserialize, Serialize};

use super::{
    appstate::SharedState,
    channel::{Channel, ChannelLike},
    guild::Guild,
    member::{Member, UserLike},
    message::Message,
    snowflake::Snowflake,
    user::{Presence, User},
};

pub trait EventLike {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>>;
    fn extract_user_id(&self) -> Option<Snowflake<User>>;
}

/// A JSON payload that can be received over the websocket by clients.
/// All events are serialized in a way such that they are wrapped in a "data" field.
#[derive(Serialize, Debug, Clone)]
#[non_exhaustive]
#[serde(tag = "event", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GatewayEvent {
    /// The initial message sent on connection.
    Hello(HelloPayload),
    /// A heartbeat acknowledgement.
    HeartbeatAck,
    /// A chat message.
    MessageCreate(Message),
    /// A peer has joined the chat.
    MemberCreate(Member),
    /// A peer has left the chat.
    MemberRemove(DeletePayload<User>),
    /// A guild was created.
    GuildCreate(GuildCreatePayload),
    /// A guild was deleted.
    GuildRemove(Guild),
    /// A channel was created.
    ChannelCreate(Channel),
    /// A channel was deleted.
    ChannelRemove(Channel),
    // A user's presence was updated.
    PresenceUpdate(PresenceUpdatePayload),
    /// The server is ready to accept messages.
    Ready(ReadyPayload),
    /// The server has closed the connection.
    InvalidSession(String),
}

// pain x_x
impl EventLike for GatewayEvent {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        match self {
            Self::Hello(_) => None,
            Self::HeartbeatAck => None,
            Self::MessageCreate(message) => message.extract_guild_id(),
            Self::MemberCreate(member) => member.extract_guild_id(),
            Self::MemberRemove(payload) => payload.extract_guild_id(),
            Self::GuildCreate(guild) => guild.extract_guild_id(),
            Self::GuildRemove(payload) => payload.extract_guild_id(),
            Self::ChannelCreate(channel) => channel.extract_guild_id(),
            Self::ChannelRemove(payload) => payload.extract_guild_id(),
            Self::PresenceUpdate(_) => None,
            Self::Ready(_) => None,
            Self::InvalidSession(_) => None,
        }
    }

    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        match self {
            Self::Hello(_) => None,
            Self::HeartbeatAck => None,
            Self::MessageCreate(message) => message.extract_user_id(),
            Self::MemberCreate(member) => member.extract_user_id(),
            Self::MemberRemove(payload) => payload.extract_user_id(),
            Self::GuildCreate(guild) => guild.extract_user_id(),
            Self::GuildRemove(payload) => payload.extract_user_id(),
            Self::ChannelCreate(channel) => channel.extract_user_id(),
            Self::ChannelRemove(payload) => payload.extract_user_id(),
            Self::PresenceUpdate(payload) => Some(payload.user_id),
            Self::Ready(payload) => payload.extract_user_id(),
            Self::InvalidSession(_) => None,
        }
    }
}

impl EventLike for Message {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        if let Some(UserLike::Member(member)) = &self.author() {
            Some(member.guild_id())
        } else {
            None
        }
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        self.author().as_ref().map(|userlike| userlike.id())
    }
}

impl EventLike for Member {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        Some(self.guild_id())
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        Some(self.user().id())
    }
}

impl EventLike for Channel {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        Some(self.guild_id())
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        None
    }
}

impl EventLike for Guild {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        Some(self.id())
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        None
    }
}

impl EventLike for User {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        None
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        Some(self.id())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HelloPayload {
    heartbeat_interval: u64,
}

impl HelloPayload {
    pub fn new(heartbeat_interval: u64) -> Self {
        Self { heartbeat_interval }
    }
}

/// A wrapper object around an ID with an optional guild_id to aid gateway event filtering.
/// The guild_id field is not serialized and sent through the API.
#[derive(Debug, Clone, Serialize)]
pub struct DeletePayload<T> {
    id: Snowflake<T>,
    #[serde(skip)]
    guild_id: Option<Snowflake<Guild>>,
}

impl<T> DeletePayload<T> {
    pub fn new(id: Snowflake<T>, guild_id: Option<Snowflake<Guild>>) -> Self {
        Self { id, guild_id }
    }
}

impl EventLike for DeletePayload<User> {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        self.guild_id
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        None
    }
}

/// Represents the payload of a `PRESENCE_UPDATE` event.
/// In other words, when the user changes their status (e.g. 'Online' to 'Offline') this is the payload received.
#[derive(Serialize, Clone, Debug)]
pub struct PresenceUpdatePayload {
    pub user_id: Snowflake<User>,
    pub presence: Presence,
}

/// Represents a `GUILD_CREATE` payload.
///
/// This event is dispatched when a new guild is created, or when initially connecting to the gateway to fill client cache.
#[derive(Serialize, Debug, Clone)]
pub struct GuildCreatePayload {
    pub guild: Guild,
    pub members: Vec<Member>,
    pub channels: Vec<Channel>,
}

impl GuildCreatePayload {
    pub fn new(guild: Guild, members: Vec<Member>, channels: Vec<Channel>) -> Self {
        Self {
            guild,
            members,
            channels,
        }
    }

    /// Create a new guild create payload by fetching all relevant data from the database.
    ///
    /// ## Locks
    ///
    /// * `app().gateway` (read)
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn from_guild(app: SharedState, guild: Guild) -> Result<Self, sqlx::Error> {
        // Presences need to be included in the payload
        let members = future::join_all(
            guild
                .fetch_members(app.clone())
                .await?
                .into_iter()
                .map(|m| m.include_presence(app.clone())),
        )
        .await;
        let channels = guild.fetch_channels(app).await?;
        Ok(Self::new(guild, members, channels))
    }
}

impl EventLike for GuildCreatePayload {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        Some(self.guild.id())
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        None
    }
}

/// A payload sent by the server to the client after a handshake.
#[derive(Serialize, Debug, Clone)]
pub struct ReadyPayload {
    pub user: User,
    pub guilds: Vec<Guild>,
}

impl ReadyPayload {
    pub fn new(user: User, guilds: Vec<Guild>) -> Self {
        Self { user, guilds }
    }
}

impl EventLike for ReadyPayload {
    fn extract_guild_id(&self) -> Option<Snowflake<Guild>> {
        None
    }
    fn extract_user_id(&self) -> Option<Snowflake<User>> {
        Some(self.user.id())
    }
}

/// A JSON payload that can be sent over the websocket by clients.
#[derive(Deserialize, Debug, Clone)]
#[non_exhaustive]
#[serde(tag = "event", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GatewayMessage {
    /// Identify with the server. This should be the first event sent by the client.
    Identify(IdentifyPayload),
    /// A heartbeat message to indicate that the client is still active.
    Heartbeat,
}

#[derive(Deserialize, Debug, Clone)]
pub struct IdentifyPayload {
    pub token: Secret<String>,
}
