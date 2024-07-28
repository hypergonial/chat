use std::{
    error::Error,
    fmt::{Debug, Display, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    num::ParseIntError,
    str::FromStr,
};

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use snowflake::SnowflakeIdGenerator;
use sqlx::{postgres::PgHasArrayType, Decode, Encode};
use std::time::SystemTime;

use super::state::Config;

// Custom epoch of 2023-01-01T00:00:00Z in miliseconds
pub const EPOCH: i64 = 1_672_531_200_000;

/// A snowflake ID used to identify entities.
///
/// Snowflakes are 64-bit integers that are guaranteed to be unique.
/// The first 41 bits are a timestamp, the next 10 are a worker ID, and the last 12 are a process ID.
#[repr(transparent)]
pub struct Snowflake<T> {
    // Note: We are using i64 instead of u64 because postgres does not support unsigned integers.
    value: i64,
    _marker: PhantomData<T>,
}

impl<T> Snowflake<T> {
    /// Create a new snowflake from a 64-bit integer.
    pub const fn new(value: i64) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }

    /// Generate a new snowflake using the current time.
    pub fn gen_new(config: &Config) -> Self {
        let mut gen = get_generator(config.machine_id(), config.process_id());
        gen.generate().into()
    }

    /// Cast this snowflake to a different marker type.
    pub const fn cast<U>(self) -> Snowflake<U> {
        Snowflake::new(self.value)
    }

    /// UNIX timestamp representing the time at which this snowflake was created in milliseconds.
    pub const fn timestamp(&self) -> i64 {
        (self.value >> 22) + EPOCH
    }

    /// Returns the creation time of this snowflake.
    pub fn created_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.timestamp(), 0).expect("Failed to convert timestamp to DateTime")
    }

    /// Returns the worker ID that generated this snowflake.
    pub const fn worker_id(&self) -> i64 {
        (self.value & 0x003E_0000) >> 17
    }

    /// Returns the process ID that generated this snowflake.
    pub const fn process_id(&self) -> i64 {
        (self.value & 0x1F000) >> 12
    }
}

impl<T> From<i64> for Snowflake<T> {
    fn from(value: i64) -> Self {
        Self::new(value)
    }
}

impl<T> From<Snowflake<T>> for i64 {
    fn from(snowflake: Snowflake<T>) -> Self {
        snowflake.value
    }
}

impl<T> Clone for Snowflake<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Snowflake<T> {}

impl<T> PartialEq for Snowflake<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Eq for Snowflake<T> {}

impl<T> Hash for Snowflake<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T> Display for Snowflake<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> Debug for Snowflake<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snowflake({})", self.value)
    }
}

impl<T> FromStr for Snowflake<T> {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i64::from_str(s).map(Self::new)
    }
}

impl<T> Serialize for Snowflake<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.to_string().serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Snowflake<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?
            .parse()
            .map_err(|_| serde::de::Error::custom("failed parsing snowflake from string"))?;
        Ok(Self::new(value))
    }
}

impl<DB: sqlx::Database, T> sqlx::Type<DB> for Snowflake<T>
where
    i64: sqlx::Type<DB>,
{
    fn type_info() -> <DB as sqlx::Database>::TypeInfo {
        <i64 as sqlx::Type<DB>>::type_info()
    }
}

impl<'q, DB: sqlx::Database, T> Encode<'q, DB> for Snowflake<T>
where
    i64: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, Box<dyn Error + Send + Sync>> {
        <i64 as sqlx::Encode<DB>>::encode_by_ref(&self.value, buf)
    }
}

impl<'r, DB: sqlx::Database, T> Decode<'r, DB> for Snowflake<T>
where
    i64: Decode<'r, DB>,
{
    fn decode(value: <DB as sqlx::Database>::ValueRef<'r>) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let value = <i64 as Decode<DB>>::decode(value)?;
        Ok(Self::new(value))
    }
}

impl<T> PgHasArrayType for Snowflake<T> {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        <i64 as PgHasArrayType>::array_type_info()
    }
}

/// Retrieve a new Snowflake ID generator that uses the custom epoch.
#[inline]
pub fn get_generator(worker_id: i32, process_id: i32) -> SnowflakeIdGenerator {
    SnowflakeIdGenerator::with_epoch(
        worker_id,
        process_id,
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(EPOCH as u64),
    )
}
