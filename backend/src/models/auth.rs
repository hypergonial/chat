use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};

use super::appstate::APP;
use super::snowflake::Snowflake;
use chrono::prelude::*;
use core::fmt::Debug;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenData {
    user_id: u64,
    /// The expiration time of the token in seconds
    exp: usize,
    /// Issued at time of the token in seconds
    iat: usize,
}

impl TokenData {
    /// Create a new token data struct with the given user id and iat
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user id to store in the token
    /// * `iat` - The issuer time of the token
    pub fn new(user_id: u64, iat: usize) -> Self {
        TokenData {
            user_id,
            iat,
            exp: Utc::now().timestamp() as usize + 86400,
        }
    }

    /// Returns the user id of the token
    pub fn user_id(&self) -> u64 {
        self.user_id
    }

    /// Returns the issuer time of the token
    pub fn iat(&self) -> usize {
        self.iat
    }

    /// Returns the expiration time of the token
    pub fn exp(&self) -> usize {
        self.exp
    }
}

/// Represents a JWT used for authentication
#[derive(Clone)]
pub struct Token {
    /// The data stored in the token
    data: TokenData,
    /// The token string
    token: Secret<String>,
}

impl Token {
    /// Generate a new token with the given data
    ///
    /// # Arguments
    ///
    /// * `data` - The data to store in the token
    /// * `secret` - The secret to sign the token with
    ///
    /// # Errors
    ///
    /// Returns an error if the token could not be generated or contains invalid data.
    pub fn new(data: &TokenData, secret: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Token {
            data: data.clone(),
            token: Secret::new(encode(
                &Header::default(),
                &data,
                &EncodingKey::from_secret(secret.as_ref()),
            )?),
        })
    }

    /// Generate a new token for the given user, with the current time as the issue time.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The id of the user to generate the token for
    /// * `secret` - The secret to sign the token with
    ///
    /// # Errors
    ///
    /// Returns an error if the token could not be generated or contains invalid data.
    pub fn new_for(user_id: Snowflake, secret: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        Self::new(
            &TokenData::new(user_id.into(), Utc::now().timestamp() as usize),
            secret,
        )
    }

    /// Decode an existing token and return it
    ///
    /// # Arguments
    ///
    /// * `token` - The token to decode
    /// * `secret` - The secret to decode the token with
    ///
    /// # Errors
    ///
    /// Returns an error if the token could not be decoded or the secret was invalid.
    pub fn decode(token: &str, secret: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        let decoded = decode::<TokenData>(
            token,
            &DecodingKey::from_secret(secret.as_ref()),
            &Validation::default(),
        )?;
        Ok(Token {
            data: decoded.claims,
            token: Secret::new(token.to_string()),
        })
    }

    /// Returns the token data
    pub fn data(&self) -> &TokenData {
        &self.data
    }
}

impl ExposeSecret<String> for Token {
    fn expose_secret(&self) -> &String {
        self.token.expose_secret()
    }
}

impl Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("data", &self.data)
            .field("token", &"**********")
            .finish()
    }
}

/// An incoming set of credentials.
#[derive(Deserialize, Debug, Clone)]
pub struct Credentials {
    username: String,
    password: Secret<String>,
}

impl Credentials {
    pub fn new(username: String, password: String) -> Self {
        Credentials {
            username,
            password: Secret::new(password),
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &Secret<String> {
        &self.password
    }
}

pub struct StoredCredentials {
    user_id: u64,
    hash: Secret<String>,
}

impl StoredCredentials {
    /// Create a new set of stored credentials.
    pub fn new(user_id: u64, hash: String) -> Self {
        StoredCredentials {
            user_id,
            hash: Secret::new(hash),
        }
    }

    /// The user id of the user that owns the credentials.
    pub fn user_id(&self) -> u64 {
        self.user_id
    }

    /// The hashed password stored in PHC string format.
    pub fn hash(&self) -> &Secret<String> {
        &self.hash
    }

    /// Fetch a set of credentials from the database.
    ///
    /// # Arguments
    ///
    /// * `username` - The username to fetch credentials for.
    ///
    /// # Returns
    ///
    /// * `Option<StoredCredentials>` - The credentials if they exist.
    pub async fn fetch(username: String) -> Option<StoredCredentials> {
        let db = &APP.read().await.db;

        let result = sqlx::query!(
            "SELECT users.id, secrets.password
            FROM users JOIN secrets ON users.id = secrets.user_id
            WHERE users.username = $1",
            username
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;

        Some(Self {
            user_id: result
                .id
                .try_into()
                .expect("user_id is negative for some reason"),
            hash: Secret::new(result.password),
        })
    }

    /// Commit the credentials to the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the credentials could not be committed,
    /// this could be due to the user not existing in the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = &APP.read().await.db;

        sqlx::query!(
            "INSERT INTO secrets (user_id, password) VALUES ($1, $2)
            ON CONFLICT (user_id) DO UPDATE SET password = $2",
            self.user_id as i64,
            self.hash.expose_secret()
        )
        .execute(db.pool())
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_token() {
        let data = TokenData {
            user_id: 123,
            iat: 123,
            exp: Utc::now().timestamp() as usize + 1000000,
        };
        let token = Token::new(&data, "among us").unwrap();
        let decoded_token = Token::decode(token.expose_secret(), "among us").unwrap();
        assert_eq!(decoded_token.data().user_id, 123);
        assert_eq!(decoded_token.data().iat, 123);
    }

    #[test]
    fn test_different_secret_fail() {
        let data = TokenData {
            user_id: 123,
            iat: 123,
            exp: Utc::now().timestamp() as usize + 1000000,
        };
        let token = Token::new(&data, "among us").unwrap();
        let err = Token::decode(token.expose_secret(), "sussage").unwrap_err();
        assert_eq!(err.to_string(), "InvalidSignature");
    }
}
