use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use secrecy::{ExposeSecret, Secret};

use crate::models::snowflake::Snowflake;
use crate::models::{
    auth::{Credentials, StoredCredentials},
    errors::AuthError,
};

/// Verify a set of credentials against the database in constant time.
///
/// # Arguments
///
/// * `credentials` - The credentials to verify.
///
/// # Returns
///
/// * [`Ok(Snowflake)`] - The user id of the user that owns the credentials.
/// * [`Err(AuthError)`] - If the credentials are invalid or the user was not found.
pub async fn validate_credentials(credentials: Credentials) -> Result<Snowflake, AuthError> {
    let mut user_id: Option<Snowflake> = None;
    // We set up a dummy hash here so verify_password_hash is always run.
    // This is to prevent timing attacks.
    let mut expected_hash = Secret::new(
        "$argon2id$v=19$m=15000,t=2,p=1$\
        gZiV/M1gPc22ElAH/Jh1Hw$\
        CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
            .to_string(),
    );

    if let Some(stored_credentials) = StoredCredentials::fetch_by_username(credentials.username().to_string()).await {
        user_id = Some(stored_credentials.user_id());
        expected_hash = stored_credentials.hash().clone();
    }

    tokio::task::spawn_blocking(move || verify_password_hash(expected_hash, credentials.password().clone()))
        .await
        .unwrap()?;

    // If the user doesn't actually exist, fail.
    if user_id.is_none() {
        return Err(AuthError::UserNotFound);
    }

    Ok(user_id.unwrap())
}

/// Verify a password candidate against a known hash.
///
/// # Arguments
///
/// * `expected_hash` - The hash to verify against, this is usually stored in a database.
/// * `password_candidate` - The password candidate to verify.
///
/// # Returns
///
/// * `Ok(())` - If the password candidate matches the hash.
/// * `Err(AuthError::WrongCredentials)` - If the password candidate does not match the hash.
/// * `Err(AuthError::PasswordHash)` - If the password candidate could not be hashed.
fn verify_password_hash(expected_hash: Secret<String>, password_candidate: Secret<String>) -> Result<(), AuthError> {
    let expected_hash = PasswordHash::new(expected_hash.expose_secret())?;
    Argon2::default()
        .verify_password(password_candidate.expose_secret().as_bytes(), &expected_hash)
        .map_err(|_| AuthError::WrongCredentials)
}

/// Generate a hash for a new password.
///
/// # Arguments
///
/// * `password` - The password to hash.
///
/// # Returns
///
/// * `Ok(String)` - The hash of the password.
/// * `Err(AuthError::PasswordHash)` - If the password could not be hashed.
pub fn generate_hash(password: &Secret<String>) -> Result<String, AuthError> {
    let hasher = Argon2::default();
    let salt = SaltString::generate(&mut rand::thread_rng());
    Ok(hasher
        .hash_password(password.expose_secret().as_bytes(), &salt)?
        .to_string())
}
