use super::{message::Message, user::User};
use serde::{Deserialize, Serialize};

/// A JSON payload that can be received over the websocket by clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub enum GatewayEvent {
    /// A chat message.
    MessageCreate(Message),
    /// A peer has joined the chat.
    MemberJoin(String),
    /// A peer has left the chat.
    MemberLeave(String),
    /// The server is ready to accept messages.
    Ready(User),
    /// The server has closed the connection.
    InvalidSession(String),
}

/// A JSON payload that can be sent over the websocket by clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub enum GatewayMessage {
    /// Identify with the server. This should be the first event sent by the client.
    Identify(String),
}
