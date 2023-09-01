use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use super::{appstate::APP, snowflake::Snowflake};

bitflags! {
    /// Boolean flags for user preferences
    #[derive(Debug, Clone, Copy)]
    pub struct PrefFlags: u64 {
        const RENDER_ATTACHMENTS = 1;
        const AUTOPLAY_GIF = 1 << 1;
    }
}

impl Default for PrefFlags {
    fn default() -> Self {
        PrefFlags::RENDER_ATTACHMENTS | PrefFlags::AUTOPLAY_GIF
    }
}

impl Serialize for PrefFlags {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.bits())
    }
}

impl<'de> Deserialize<'de> for PrefFlags {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let flags = u64::deserialize(deserializer)?;
        Ok(PrefFlags::from_bits(flags).unwrap_or_default())
    }
}

/// Layout for frontend UI
#[derive(Debug, Clone, Copy)]
pub enum Layout {
    Compact = 0,
    Normal = 1,
    Comfy = 2,
}

impl<T: Into<u8>> From<T> for Layout {
    fn from(layout: T) -> Self {
        match layout.into() {
            0 => Layout::Compact,
            1 => Layout::Normal,
            2 => Layout::Comfy,
            _ => Layout::Normal,
        }
    }
}

impl Serialize for Layout {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let layout = u8::deserialize(deserializer)?;
        Ok(Layout::from(layout))
    }
}

/// Update payload for user preferences
#[derive(Debug, Clone, Deserialize)]
pub struct PrefsUpdate {
    pub flags: Option<PrefFlags>,
    pub message_grouping_timeout: Option<u64>,
    pub layout: Option<Layout>,
    pub text_size: Option<u8>,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Prefs {
    #[serde(skip)]
    user_id: Snowflake,
    /// The user's preferences flags.
    pub flags: PrefFlags,
    /// The timeout for grouping messages in seconds.
    pub message_grouping_timeout: u64,
    /// The layout of the frontend.
    pub layout: Layout,
    /// The text size of chat messages.
    pub text_size: u8,
    /// The date format for chat messages.
    pub locale: String,
}

impl Prefs {
    pub fn new(user_id: Snowflake) -> Self {
        Prefs {
            user_id,
            flags: PrefFlags::default(),
            message_grouping_timeout: 60,
            layout: Layout::Normal,
            text_size: 12,
            locale: String::from("en_US"),
        }
    }

    /// The user id of the user that owns the preferences.
    pub fn user_id(&self) -> Snowflake {
        self.user_id
    }

    /// Apply a set of updates to the preferences.
    pub fn update(&mut self, update: PrefsUpdate) {
        if let Some(flags) = update.flags {
            self.flags = flags;
        }
        if let Some(message_grouping_timeout) = update.message_grouping_timeout {
            self.message_grouping_timeout = message_grouping_timeout;
        }
        if let Some(layout) = update.layout {
            self.layout = layout;
        }
        if let Some(text_size) = update.text_size {
            self.text_size = text_size;
        }
        if let Some(locale) = update.locale {
            self.locale = locale;
        }
    }

    /// Fetch the preferences for a user.
    ///
    /// ## Arguments
    ///
    /// * `user` - The user to fetch preferences for.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch(user: impl Into<Snowflake>) -> Result<Self, sqlx::Error> {
        let db = APP.db.read().await;
        let user_id: Snowflake = user.into();
        let user_id_i64: i64 = user_id.into();

        let result = sqlx::query!(
            "SELECT user_id, flags, message_grouping_timeout, layout, text_size, locale
            FROM prefs
            WHERE user_id = $1",
            user_id_i64
        )
        .fetch_optional(db.pool())
        .await?;

        if result.is_none() {
            return Ok(Self::new(user_id));
        }

        let result = result.unwrap();

        Ok(Self {
            user_id,
            flags: PrefFlags::from_bits(result.flags.try_into().unwrap()).unwrap_or_default(),
            message_grouping_timeout: result.message_grouping_timeout as u64,
            layout: Layout::from(result.layout as u8),
            text_size: result.text_size as u8,
            locale: result.locale,
        })
    }

    /// Commit the preferences to the database.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = APP.db.read().await;
        let user_id: i64 = self.user_id.into();
        let flags: i64 = self.flags.bits().try_into().expect("Cannot fit flag into i64");

        sqlx::query!(
            "INSERT INTO prefs (user_id, flags, message_grouping_timeout, layout, text_size, locale)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id)
            DO UPDATE SET flags = $2, message_grouping_timeout = $3, layout = $4, text_size = $5, locale = $6",
            user_id,
            flags,
            self.message_grouping_timeout as i32,
            self.layout as i32,
            self.text_size as i32,
            self.locale,
        )
        .execute(db.pool())
        .await?;

        Ok(())
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            user_id: Snowflake::default(),
            flags: PrefFlags::default(),
            message_grouping_timeout: 60,
            layout: Layout::Normal,
            text_size: 12,
            locale: String::from("en_US"),
        }
    }
}
