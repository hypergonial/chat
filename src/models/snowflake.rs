use std::fmt::Display;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use snowflake::SnowflakeIdGenerator;
use std::time::SystemTime;

// Custom epoch of 2023-01-01T00:00:00Z
pub const EPOCH: u64 = 1672531200;

/// A snowflake ID used to identify entities.
///
/// Snowflakes are 64-bit unsigned integers that are guaranteed to be unique.
/// The first 41 bits are a timestamp, the next 10 are a worker ID, and the last 12 are a process ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Snowflake {
    value: u64,
}

impl Snowflake {
    pub fn new(value: u64) -> Self {
        Snowflake { value }
    }

    /// UNIX timestamp representing the time at which this snowflake was created in milliseconds.
    pub fn timestamp(&self) -> u64 {
        (self.value >> 22) + EPOCH
    }

    /// Returns the creation time of this snowflake.
    pub fn created_at(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from_utc(
            NaiveDateTime::from_timestamp_opt(self.timestamp() as i64, 0).unwrap(),
            Utc,
        )
    }

    /// Returns the worker ID that generated this snowflake.
    pub fn worker_id(&self) -> u64 {
        (self.value & 0x3E0000) >> 17
    }

    /// Returns the process ID that generated this snowflake.
    pub fn process_id(&self) -> u64 {
        (self.value & 0x1F000) >> 12
    }
}

impl From<u64> for Snowflake {
    fn from(value: u64) -> Self {
        Snowflake::new(value)
    }
}

impl From<Snowflake> for u64 {
    fn from(snowflake: Snowflake) -> Self {
        snowflake.value
    }
}

impl From<i64> for Snowflake {
    fn from(value: i64) -> Self {
        Snowflake::new(value as u64)
    }
}

impl From<Snowflake> for i64 {
    fn from(snowflake: Snowflake) -> Self {
        snowflake.value as i64
    }
}

impl Display for Snowflake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl sqlx::Type<sqlx::Postgres> for Snowflake {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <i64 as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for Snowflake {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <i64 as sqlx::Encode<sqlx::Postgres>>::encode(self.value as i64, buf)
    }
}

// implement serialization as a u64
impl Serialize for Snowflake {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

// implement deserialization from a u64
impl<'de> Deserialize<'de> for Snowflake {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(deserializer)?;
        Ok(Snowflake::new(value))
    }
}

/// Retrieve a new Snowflake ID generator that uses the custom epoch.
#[inline]
pub fn get_generator(worker_id: i32, process_id: i32) -> SnowflakeIdGenerator {
    SnowflakeIdGenerator::with_epoch(
        worker_id,
        process_id,
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(EPOCH),
    )
}
