use serde::{Deserialize, Serialize};
use super::message::Message;

/// A JSON payload that can be sent over the websocket.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SocketEvent {
    /// A chat message.
    MessageCreate(Message),
    /// A peer has joined the chat.
    MemberJoin(String),
    /// A peer has left the chat.
    MemberLeave(String),
}
