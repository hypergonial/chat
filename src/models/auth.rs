use core::fmt::Debug;

use axum::{extract::FromRequestParts, http::request::Parts, RequestPartsExt};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use chrono::prelude::*;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};

use super::{
    errors::{AuthError, RESTError},
    snowflake::Snowflake,
    state::App,
    user::User,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenData {
    /// The user id of the token owner
    user_id: Snowflake<User>,
    /// The expiration time of the token in seconds
    /// Note: This field is validated by the jsonwebtoken crate
    exp: usize,
    /// Issued at time of the token in seconds
    /// Note: This field is validated by the jsonwebtoken crate
    iat: usize,
}

impl TokenData {
    /// Create a new token data struct with the given user id and iat
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user id to store in the token
    /// * `iat` - The issuer time of the token
    fn new(user_id: Snowflake<User>, iat: usize) -> Self {
        Self {
            user_id,
            iat,
            exp: Utc::now().timestamp() as usize + 86400,
        }
    }

    /// Returns the user id of the token owner
    pub const fn user_id(&self) -> Snowflake<User> {
        self.user_id
    }

    /// Returns the issuer time of the token
    pub const fn iat(&self) -> usize {
        self.iat
    }

    /// Returns the expiration time of the token
    pub const fn exp(&self) -> usize {
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
    /// [`jsonwebtoken::errors::Error`] - If the token could not be generated.
    fn new(secret: &Secret<String>, data: &TokenData) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            data: data.clone(),
            token: Secret::new(encode(
                &Header::default(),
                &data,
                &EncodingKey::from_secret(secret.expose_secret().as_ref()),
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
    /// [`jsonwebtoken::errors::Error`] - If the token could not be generated.
    pub fn new_for(secret: &Secret<String>, user_id: Snowflake<User>) -> Result<Self, jsonwebtoken::errors::Error> {
        Self::new(secret, &TokenData::new(user_id, Utc::now().timestamp() as usize))
    }

    /// Decode an existing token and return it. This will not validate the token.
    ///
    /// # Arguments
    ///
    /// * `token` - The token to decode
    /// * `secret` - The secret to decode the token with
    ///
    /// # Errors
    ///
    /// [`jsonwebtoken::errors::Error`] - If the token could not be decoded.
    fn decode(secret: &Secret<String>, token: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        let decoded = decode::<TokenData>(
            token,
            &DecodingKey::from_secret(secret.expose_secret().as_ref()),
            &Validation::default(),
        )?;
        Ok(Self {
            data: decoded.claims,
            token: Secret::new(token.to_string()),
        })
    }

    /// Decode and validate an existing token and return it.
    ///
    /// # Arguments
    ///
    /// * `token` - The token to decode
    /// * `secret` - The secret to decode the token with
    ///
    /// # Errors
    ///
    /// [`jsonwebtoken::errors::Error`] - If the token could not be decoded.
    /// [`AuthError::InvalidToken`] - If the token is invalid.
    /// [`RESTError::NotFound`] - If the user entry for the token could not be found.
    pub async fn validate(app: App, token: &str) -> Result<Self, RESTError> {
        let token = Self::decode(app.config.app_secret(), token)?;
        let stored_creds = StoredCredentials::fetch(app, token.data().user_id())
            .await
            .ok_or(RESTError::NotFound("User entry for token not found".into()))?;
        // Check that the token's iat is after the last changed time of the stored credentials
        if token.data().iat() < stored_creds.last_changed.timestamp() as usize {
            return Err(AuthError::InvalidToken.into());
        }
        Ok(token)
    }

    /// Returns the token data
    pub const fn data(&self) -> &TokenData {
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

/// Token extractor for axum.
#[async_trait::async_trait]
impl FromRequestParts<App> for Token {
    type Rejection = RESTError;

    /// Extract a token from request Authorization header
    async fn from_request_parts(parts: &mut Parts, state: &App) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::MissingCredentials)?;
        // Decode the user data
        Self::validate(state.clone(), bearer.token()).await
    }
}

/// An incoming set of credentials.
#[derive(Deserialize, Debug, Clone)]
pub struct Credentials {
    username: String,
    password: Secret<String>,
}

impl Credentials {
    /// Create a new set of credentials.
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password: Secret::new(password),
        }
    }

    /// The username belonging to this set of credentials.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// The password belonging to this set of credentials.
    pub const fn password(&self) -> &Secret<String> {
        &self.password
    }
}

/// Credentials, as stored in the DB
pub struct StoredCredentials {
    user_id: Snowflake<User>,
    hash: Secret<String>,
    last_changed: DateTime<Utc>,
}

impl StoredCredentials {
    /// Create a new set of stored credentials.
    pub fn new(user: impl Into<Snowflake<User>>, hash: String) -> Self {
        Self {
            user_id: user.into(),
            hash: Secret::new(hash),
            last_changed: Utc::now(),
        }
    }

    /// The user id of the user that owns the credentials.
    pub const fn user_id(&self) -> Snowflake<User> {
        self.user_id
    }

    /// The hashed password stored in PHC string format.
    pub const fn hash(&self) -> &Secret<String> {
        &self.hash
    }

    /// Fetch a set of credentials from the database.
    ///
    /// # Arguments
    ///
    /// * `user` - The user to fetch credentials for.
    ///
    /// # Returns
    ///
    /// * `Option<StoredCredentials>` - The credentials if they exist.
    pub async fn fetch(app: App, user: impl Into<Snowflake<User>>) -> Option<Self> {
        let user_id: i64 = user.into().into();

        let result = sqlx::query!(
            "SELECT user_id, password, last_changed
            FROM secrets
            WHERE user_id = $1",
            user_id
        )
        .fetch_optional(app.db.pool())
        .await
        .ok()??;

        Some(Self {
            user_id: result.user_id.into(),
            hash: Secret::new(result.password),
            last_changed: DateTime::from_timestamp(result.last_changed, 0)
                .expect("Failed to create DateTime from timestamp"),
        })
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
    pub async fn fetch_by_username(app: App, username: String) -> Option<Self> {
        let result = sqlx::query!(
            "SELECT users.id, secrets.password, secrets.last_changed
            FROM users JOIN secrets ON users.id = secrets.user_id
            WHERE users.username = $1",
            username
        )
        .fetch_optional(app.db.pool())
        .await
        .ok()??;

        Some(Self {
            user_id: result.id.into(),
            hash: Secret::new(result.password),
            last_changed: DateTime::from_timestamp(result.last_changed, 0)
                .expect("Failed to create DateTime from timestamp"),
        })
    }

    /// Commit the credentials to the database.
    ///
    /// # Errors
    ///
    /// * [`sqlx::Error`] - If the query fails. This could be due to the user not existing in the DB.
    pub async fn commit(&self, app: App) -> Result<(), sqlx::Error> {
        let user_id: i64 = self.user_id.into();

        sqlx::query!(
            "INSERT INTO secrets (user_id, password, last_changed) VALUES ($1, $2, $3)
            ON CONFLICT (user_id) DO UPDATE SET password = $2, last_changed = $3",
            user_id,
            self.hash.expose_secret(),
            self.last_changed.timestamp()
        )
        .execute(app.db.pool())
        .await?;

        Ok(())
    }

    /// Update the password hash of the credentials, changing the last changed field with it.
    pub fn update_hash(&mut self, new_hash: Secret<String>) {
        self.hash = new_hash;
        self.last_changed = Utc::now();
    }
}
