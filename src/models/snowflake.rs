use std::{fmt::Display, hash::Hash, marker::PhantomData, num::ParseIntError, str::FromStr};

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use snowflake::SnowflakeIdGenerator;
use std::time::SystemTime;

use super::appstate::SharedState;

// Custom epoch of 2023-01-01T00:00:00Z in miliseconds
pub const EPOCH: i64 = 1672531200000;

/// A snowflake ID used to identify entities.
///
/// Snowflakes are 64-bit integers that are guaranteed to be unique.
/// The first 41 bits are a timestamp, the next 10 are a worker ID, and the last 12 are a process ID.
#[derive(Debug)]
pub struct Snowflake<T> {
    // Note: We are using i64 instead of u64 because postgres does not support unsigned integers.
    value: i64,
    _marker: PhantomData<T>,
}

impl<T> Snowflake<T> {
    /// Create a new snowflake from a 64-bit integer.
    pub fn new(value: i64) -> Self {
        Snowflake {
            value,
            _marker: PhantomData,
        }
    }

    /// Generate a new snowflake using the current time.
    pub fn gen_new(app: SharedState) -> Self {
        let mut gen = get_generator(app.config.machine_id(), app.config.process_id());
        gen.generate().into()
    }

    /// UNIX timestamp representing the time at which this snowflake was created in milliseconds.
    pub fn timestamp(&self) -> i64 {
        (self.value >> 22) + EPOCH
    }

    /// Returns the creation time of this snowflake.
    pub fn created_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.timestamp(), 0).unwrap()
    }

    /// Returns the worker ID that generated this snowflake.
    pub fn worker_id(&self) -> i64 {
        (self.value & 0x3E0000) >> 17
    }

    /// Returns the process ID that generated this snowflake.
    pub fn process_id(&self) -> i64 {
        (self.value & 0x1F000) >> 12
    }
}

impl<T> From<i64> for Snowflake<T> {
    fn from(value: i64) -> Self {
        Snowflake::new(value)
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
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T> Display for Snowflake<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> FromStr for Snowflake<T> {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i64::from_str(s).map(Snowflake::new)
    }
}

// implement serialization as a i64
impl<T> Serialize for Snowflake<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.to_string().serialize(serializer)
    }
}

// implement deserialization from a i64
impl<'de, T> Deserialize<'de> for Snowflake<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?
            .parse()
            .map_err(|_| serde::de::Error::custom("failed parsing snowflake from string"))?;
        Ok(Snowflake::new(value))
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
