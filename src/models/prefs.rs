use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use super::{requests::UpdatePrefs, snowflake::Snowflake, state::App, user::User};

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
        Self::RENDER_ATTACHMENTS | Self::AUTOPLAY_GIF
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
        Ok(Self::from_bits(flags).unwrap_or_default())
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
            0 => Self::Compact,
            2 => Self::Comfy,
            _ => Self::Normal,
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
        Ok(Self::from(layout))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Prefs {
    #[serde(skip)]
    user_id: Snowflake<User>,
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
    pub fn new(user_id: Snowflake<User>) -> Self {
        Self {
            user_id,
            flags: PrefFlags::default(),
            message_grouping_timeout: 60,
            layout: Layout::Normal,
            text_size: 12,
            locale: String::from("en_US"),
        }
    }

    /// The user id of the user that owns the preferences.
    pub const fn user_id(&self) -> Snowflake<User> {
        self.user_id
    }

    /// Apply a set of updates to the preferences.
    pub fn update(&mut self, update: UpdatePrefs) {
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
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch(app: App, user: impl Into<Snowflake<User>>) -> Result<Self, sqlx::Error> {
        let user_id: Snowflake<User> = user.into();
        let user_id_i64: i64 = user_id.into();

        let result = sqlx::query!(
            "SELECT user_id, flags, message_grouping_timeout, layout, text_size, locale
            FROM prefs
            WHERE user_id = $1",
            user_id_i64
        )
        .fetch_optional(app.db.pool())
        .await?;

        let Some(result) = result else {
            return Ok(Self::new(user_id));
        };

        Ok(Self {
            user_id,
            flags: PrefFlags::from_bits(result.flags.try_into().expect("Failed to fit PrefFlags into u64"))
                .unwrap_or_default(),
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
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn commit(&self, app: App) -> Result<(), sqlx::Error> {
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
            i16::from(self.text_size),
            self.locale,
        )
        .execute(app.db.pool())
        .await?;

        Ok(())
    }
}

/* impl Default for Prefs {
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
 */
