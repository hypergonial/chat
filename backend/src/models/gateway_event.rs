use super::{message::Message, user::User};
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
    MemberJoin(User),
    /// A peer has left the chat.
    MemberLeave(User),
    /// The server is ready to accept messages.
    Ready(User),
    /// The server has closed the connection.
    InvalidSession(String),
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
