use serde::{Deserialize, Serialize};
use super::user::User;

/// A chat message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// The author of the message.
    pub author: User,
    /// The content of the message.
    pub content: String,
}
