/* use std::{
    collections::HashSet,
    sync::{Arc, Weak},
};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::models::{
    gateway_event::{GatewayEvent, GatewayMessage},
    guild::Guild,
    snowflake::Snowflake,
    state::ApplicationState,
    user::User,
};

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum GatewayCloseCode {
    /// Successful operation / regular socket shutdown
    Normal = 1000,
    /// Client/Server is leaving (browser tab closing, server shutting down, etc.)
    GoingAway = 1001,
    /// Endpoint received a malformed frame
    ProtocolError = 1002,
    /// Endpoint received an unsupported frame (e.g. binary-only endpoint received text frame)
    Unsupported = 1003,
    /// Expected close status, received none
    NoStatus = 1005,
    /// No close code frame has been receieved
    Abnormal = 1006,
    /// Endpoint received inconsistent message (e.g. malformed UTF-8)
    InvalidPayload = 1007,
    ///Generic code used for situations other than 1003 and 1009
    PolicyViolation = 1008,
    /// Endpoint won't process large frame
    TooLarge = 1009,
    /// Client wanted an extension which server did not negotiate
    ExtensionRequired = 1010,
    /// Internal server error while operating
    ServerError = 1011,
    /// Server/service is restarting
    ServiceRestart = 1012,
    /// Temporary server condition forced blocking client's request
    TryAgainLater = 1013,
    /// Server acting as gateway received an invalid response
    BadGateway = 1014,
    /// Transport Layer Security handshake failure
    TLSHandshakeFail = 1015,
}

impl From<GatewayCloseCode> for u16 {
    fn from(value: GatewayCloseCode) -> Self {
        value as Self
    }
}

impl From<u16> for GatewayCloseCode {
    fn from(value: u16) -> Self {
        match value {
            1000 => Self::Normal,
            1001 => Self::GoingAway,
            1002 => Self::ProtocolError,
            1003 => Self::Unsupported,
            1005 => Self::NoStatus,
            1006 => Self::Abnormal,
            1007 => Self::InvalidPayload,
            1008 => Self::PolicyViolation,
            1009 => Self::TooLarge,
            1010 => Self::ExtensionRequired,
            1012 => Self::ServiceRestart,
            1013 => Self::TryAgainLater,
            1014 => Self::BadGateway,
            1015 => Self::TLSHandshakeFail,
            _ => Self::ServerError,
        }
    }
}

impl Serialize for GatewayCloseCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (*self as u16).serialize(serializer)
    }
}

#[derive(Debug)]
pub enum SendMode {
    ToUser(Snowflake<User>),
    ToUserGuilds(Snowflake<User>, HashSet<Snowflake<Guild>>),
    ToGuild(Snowflake<Guild>),
}

/// Possible responses issued by the server to a client
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GatewayResponse {
    // If sent through a connection handle, the payload should be sent to the client
    Event(GatewayEvent),
    // If sent through a connection handle, the connection should be closed
    Close(GatewayCloseCode, String),
}

pub struct Gateway {
    sender: broadcast::Sender<Arc<(GatewayResponse, SendMode)>>,
    app: Weak<ApplicationState>,
}

impl Gateway {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            sender,
            app: Weak::new(),
        }
    }

    pub fn bind_to(&mut self, app: Weak<ApplicationState>) {
        self.app = app;
    }

    pub fn app(&self) -> Arc<ApplicationState> {
        self.app.upgrade().expect("Application state was dropped")
    }

    fn subscribe(&self) -> broadcast::Receiver<Arc<(GatewayResponse, SendMode)>> {
        self.sender.subscribe()
    }

    async fn get_handle(&self, user: impl Into<Snowflake<User>>) -> ConnectionHandle {
        let user_id = user.into();

        let guild_ids = self
            .app()
            .ops()
            .fetch_guild_ids_for(user_id)
            .await
            .expect("Failed to fetch guilds for user");

        ConnectionHandle::new(user_id, guild_ids.into_iter().collect::<HashSet<_>>(), self.subscribe())
    }

    /// Dispatch an event to all users in a specific guild
    ///
    /// # Arguments
    ///
    /// * `guild` - The guild to dispatch the event to
    /// * `event` - The event to dispatch
    pub fn dispatch_to_guild(&self, guild: impl Into<Snowflake<Guild>>, event: GatewayEvent) {
        let resp = Arc::new((GatewayResponse::Event(event), SendMode::ToGuild(guild.into())));

        if let Err(e) = self.sender.send(resp) {
            tracing::error!(error = %e, "Failed to dispatch event");
        }
    }

    /// Dispatch an event to a specific user
    ///
    /// # Arguments
    ///
    /// * `user` - The user to dispatch the event to
    /// * `event` - The event to dispatch
    pub fn dispatch_to(&self, user: impl Into<Snowflake<User>>, event: GatewayEvent) {
        let resp = Arc::new((GatewayResponse::Event(event), SendMode::ToUser(user.into())));

        if let Err(e) = self.sender.send(resp) {
            tracing::error!(error = %e, "Failed to dispatch event");
        }
    }

    /// Dispatch an event to all users in all guilds the passed user is in
    ///
    /// # Arguments
    ///
    /// * `user` - The user to dispatch the event to
    /// * `event` - The event to dispatch
    pub async fn dispatch_to_adjacent(&self, user: impl Into<Snowflake<User>>, event: GatewayEvent) {
        let user_id = user.into();

        let guild_ids = self
            .app()
            .ops()
            .fetch_guild_ids_for(user_id)
            .await
            .expect("Failed to fetch guilds for user");

        let resp = Arc::new((
            GatewayResponse::Event(event),
            SendMode::ToUserGuilds(user_id, guild_ids.into_iter().collect::<HashSet<_>>()),
        ));

        if let Err(e) = self.sender.send(resp) {
            tracing::error!(error = %e, "Failed to dispatch event");
        }
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}

struct ConnectionHandle {
    user_id: Snowflake<User>,
    guilds: HashSet<Snowflake<Guild>>,
    broadcaster: broadcast::Sender<Arc<GatewayMessage>>,
    receiver: broadcast::Receiver<Arc<(GatewayResponse, SendMode)>>,
}

impl ConnectionHandle {
    fn new(
        user_id: impl Into<Snowflake<User>>,
        guilds: HashSet<Snowflake<Guild>>,
        receiver: broadcast::Receiver<Arc<(GatewayResponse, SendMode)>>,
    ) -> Self {
        let (broadcaster, _) = broadcast::channel(16);

        Self {
            user_id: user_id.into(),
            guilds,
            broadcaster,
            receiver,
        }
    }
}
 */
