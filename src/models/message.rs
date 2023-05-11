use serde::{Deserialize, Serialize};

/// A chat message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// The author of the message.
    author: String,
    /// The content of the message.
    content: String,
}
