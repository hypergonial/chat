use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    id: usize,
    pub username: String,
}

impl User {
    pub fn new(id: usize, username: String) -> Self {
        User {
            id,
            username,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

impl Hash for User {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}