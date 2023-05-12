use super::user::User;
use serde::{Deserialize, Serialize};

/// A chat message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// The author of the message.
    pub author: User,
    /// The content of the message.
    pub content: String,
}
