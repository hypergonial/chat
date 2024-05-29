use std::{collections::HashSet, sync::Arc, time::Duration};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    broadcast,
    mpsc::{self, error::SendError},
    Mutex,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Default heartbeat interval in milliseconds
const HEARTBEAT_INTERVAL: u64 = 45000;

use crate::models::{
    appstate::SharedState,
    auth::Token,
    gateway_event::{
        EventLike, GatewayEvent, GatewayMessage, GuildCreatePayload, HelloPayload, PresenceUpdatePayload, ReadyPayload,
    },
    guild::Guild,
    snowflake::Snowflake,
    user::{Presence, User},
};

/// Possible responses issued by the server to a client
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum GatewayResponse {
    // If sent through a connection handle, the payload should be sent to the client
    Event(Arc<GatewayEvent>),
    // If sent through a connection handle, the connection should be closed
    Close(GatewayCloseCode, String),
}

/// Possible requests issued by the client to the server
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum GatewayRequest {
    // If sent through a connection handle, the payload should be broadcasted
    Message(GatewayMessage),
}

#[derive(Debug, Clone, Copy)]
pub enum GatewayCloseCode {
    /// Successful operation / regular socket shutdown
    Normal = 1000,
    /// Client is leaving (browser tab closing)
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
        value as u16
    }
}

impl From<u16> for GatewayCloseCode {
    fn from(value: u16) -> Self {
        match value {
            1000 => GatewayCloseCode::Normal,
            1001 => GatewayCloseCode::GoingAway,
            1002 => GatewayCloseCode::ProtocolError,
            1003 => GatewayCloseCode::Unsupported,
            1005 => GatewayCloseCode::NoStatus,
            1006 => GatewayCloseCode::Abnormal,
            1007 => GatewayCloseCode::InvalidPayload,
            1008 => GatewayCloseCode::PolicyViolation,
            1009 => GatewayCloseCode::TooLarge,
            1010 => GatewayCloseCode::ExtensionRequired,
            1011 => GatewayCloseCode::ServerError,
            1012 => GatewayCloseCode::ServiceRestart,
            1013 => GatewayCloseCode::TryAgainLater,
            1014 => GatewayCloseCode::BadGateway,
            1015 => GatewayCloseCode::TLSHandshakeFail,
            _ => GatewayCloseCode::Abnormal,
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

/// A struct containing connection details for a user
///
/// ## Fields
///
/// * `sender` - The sender for sending messages to the client
/// * `receiver` - The receiver for receiving messages from the client
/// * `guild_ids` - The guilds the user is a member of, this is used to filter events
#[derive(Debug, Clone)]
struct ConnectionHandle {
    sender: mpsc::UnboundedSender<GatewayResponse>,
    broadcaster: Arc<broadcast::Sender<GatewayMessage>>,
    guild_ids: HashSet<Snowflake>,
}

impl ConnectionHandle {
    /// Create a new connection handle with the given sender and guilds
    ///
    /// ## Arguments
    ///
    /// * `sender` - The sender for sending messages to the client
    /// * `guilds` - The guilds the user is a member of
    pub fn new(
        sender: mpsc::UnboundedSender<GatewayResponse>,
        receiver: Arc<broadcast::Sender<GatewayMessage>>,
        guilds: HashSet<Snowflake>,
    ) -> Self {
        ConnectionHandle {
            sender,
            broadcaster: receiver,
            guild_ids: guilds,
        }
    }

    /// Send a message to the client
    ///
    /// ## Arguments
    ///
    /// * `message` - The message to send
    pub fn send(&self, message: Arc<GatewayEvent>) -> Result<(), SendError<GatewayResponse>> {
        let resp = GatewayResponse::Event(message);
        self.sender.send(resp)
    }

    /// Close the connection with the given code and reason
    /// This will also remove the handle from the gateway state
    ///
    /// ## Arguments
    ///
    /// * `code` - The close code to send
    /// * `reason` - The reason for closing the connection
    pub fn close(&self, code: GatewayCloseCode, reason: String) -> Result<(), SendError<GatewayResponse>> {
        let resp = GatewayResponse::Close(code, reason);
        self.sender.send(resp)
    }

    /// Get a receiver for incoming gateway messages from the user
    pub fn get_receiver(&self) -> broadcast::Receiver<GatewayMessage> {
        self.broadcaster.subscribe()
    }

    /// Get the guilds the user is a member of
    pub fn guild_ids(&self) -> &HashSet<Snowflake> {
        &self.guild_ids
    }

    /// Get a mutable handle to the guilds the user is a member of
    pub fn guild_ids_mut(&mut self) -> &mut HashSet<Snowflake> {
        &mut self.guild_ids
    }
}

/// A singleton representing the gateway state
#[derive(Debug, Clone)]
pub struct Gateway {
    /// A map of currently connected users
    /// The key is the user's ID
    peers: DashMap<Snowflake, ConnectionHandle>,
}

impl Gateway {
    pub fn new() -> Self {
        Gateway { peers: DashMap::new() }
    }

    /// Add a new connection handle to the gateway state
    ///
    /// ## Arguments
    ///
    /// * `user_id` - The ID of the user to add
    /// * `handle` - The connection handle to add
    ///
    /// ## Locks
    ///
    /// * `peers` (write)
    fn add_handle(&self, user_id: Snowflake, handle: ConnectionHandle) {
        self.peers.insert(user_id, handle);
    }

    /// Remove a connection handle from the gateway state
    ///
    /// ## Arguments
    ///
    /// * `user_id` - The ID of the user to remove
    ///
    /// ## Locks
    ///
    /// * `peers` (write)
    fn remove_handle(&self, user_id: Snowflake) {
        self.peers.remove(&user_id);
    }

    /// Dispatch a new event originating from the given user to all other users
    ///
    /// ## Arguments
    ///
    /// * `payload` - The event payload
    ///
    /// ## Locks
    ///
    /// * `peers` (write)
    pub fn dispatch(&self, event: GatewayEvent) {
        tracing::debug!("Dispatching event: {:?}", event);

        // TODO: Figure out how to use the `HashMap::retain` method here without killing borrowck
        let mut to_drop: Vec<Snowflake> = Vec::new();

        // Avoid cloning the event for each user
        let event: Arc<GatewayEvent> = Arc::new(event);

        for peer in self.peers.iter() {
            let (uid, handle) = peer.pair();
            // If the event is guild-specific, only send it to users that are members of that guild
            if let Some(event_guild) = event.extract_guild_id() {
                if !handle.guild_ids().contains(&event_guild) {
                    continue;
                }
            }
            // Avoid sending events to users that don't share any guilds with the event originator
            else if let Some(user_id) = event.extract_user_id() {
                if !self.shares_guilds_with(*uid, user_id) {
                    continue;
                }
            }

            if let Err(_disconnected) = handle.send(event.clone()) {
                tracing::warn!("Error dispatching event to user: {}", uid);
                to_drop.push(*uid);
            }
        }

        for uid in to_drop {
            self.remove_handle(uid);
        }
    }

    pub fn drop_session(&self, user_id: Snowflake, code: GatewayCloseCode, reason: String) {
        if let Some(handle) = self.peers.get(&user_id) {
            handle.close(code, reason).ok();
        }
    }

    /// Registers a new guild member instance to an existing connection
    ///
    /// ## Arguments
    ///
    /// * `user` - The user to add to the connection
    /// * `guild` - The guild to add the user to
    ///
    /// ## Locks
    ///
    /// * `peers` (write)
    pub fn add_member(&self, user: impl Into<Snowflake>, guild: impl Into<Snowflake>) {
        if let Some(mut handle) = self.peers.get_mut(&user.into()) {
            handle.guild_ids_mut().insert(guild.into());
        }
    }

    /// Removes a guild member instance from an existing connection
    ///
    /// ## Arguments
    ///
    /// * `user` - The user to remove from the connection
    /// * `guild` - The guild to remove the user from
    ///
    /// ## Locks
    ///
    /// * `peers` (write)
    pub fn remove_member(&self, user: impl Into<Snowflake>, guild: impl Into<Snowflake>) {
        if let Some(mut handle) = self.peers.get_mut(&user.into()) {
            handle.guild_ids_mut().remove(&guild.into());
        }
    }

    /// Send an event to a specific user. If they are not connected, the event is dropped.
    ///
    /// ## Arguments
    ///
    /// * `user` - The user to send the event to
    /// * `event` - The event to send
    ///
    /// ## Locks
    ///
    /// * `peers` (write)
    pub fn send_to(&self, user: impl Into<Snowflake>, event: GatewayEvent) {
        let user_id: Snowflake = user.into();
        if let Some(handle) = self.peers.get(&user_id) {
            if let Err(_disconnected) = handle.send(Arc::new(event)) {
                // Drop handle to prevent deadlock
                std::mem::drop(handle);
                tracing::warn!("Error sending event to user: {}", user_id);
                self.remove_handle(user_id);
            }
        }
    }

    /// Query if a given user is connected
    ///
    /// ## Arguments
    ///
    /// * `user` - The user to check for
    ///
    /// ## Returns
    ///
    /// `true` if the user is connected, `false` otherwise
    ///
    /// ## Locks
    ///
    /// * `peers` (read)
    pub fn is_connected(&self, user: impl Into<Snowflake>) -> bool {
        self.peers.contains_key(&user.into())
    }

    /// Determines if a given user shares any guilds with another user
    ///
    /// ## Arguments
    ///
    /// * `a` - The first user to check
    /// * `b` - The second user to check
    ///
    /// ## Returns
    ///
    /// `true` if the users share any guilds, `false` otherwise
    ///
    /// ## Locks
    ///
    /// * `peers` (read)
    pub fn shares_guilds_with(&self, a: Snowflake, b: Snowflake) -> bool {
        if let Some(a_handle) = self.peers.get(&a) {
            if let Some(b_handle) = self.peers.get(&b) {
                return a_handle.guild_ids().intersection(b_handle.guild_ids()).next().is_some();
            }
        }
        false
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}

/// Get router for handling the gateway
///
/// ## Returns
///
/// A filter that can be used to handle the gateway
pub fn get_router() -> Router<SharedState> {
    Router::new().route("/", get(websocket_handler))
}

async fn websocket_handler(State(app): State<SharedState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move { handle_connection(app, socket).await })
}

/// Send HELLO, then wait for and validate the IDENTIFY payload
///
/// ## Arguments
///
/// * `ws_sink` - The sink for sending messages to the client
/// * `ws_stream` - The stream for receiving messages from the client
///
/// ## Returns
///
/// The resolved user if the handshake was successful
async fn handle_handshake(
    app: SharedState,
    ws_sink: &mut SplitSink<WebSocket, Message>,
    ws_stream: &mut SplitStream<WebSocket>,
) -> Result<User, ()> {
    // Send HELLO with the heartbeat interval
    ws_sink
        .send(Message::Text(
            serde_json::to_string(&GatewayEvent::Hello(HelloPayload::new(HEARTBEAT_INTERVAL)))
                .expect("Failed to serialize HELLO payload"),
        ))
        .await
        .ok();

    // IDENTIFY should be the first message sent
    let Some(Ok(maybe_ident)) = ws_stream.next().await else {
        ws_sink
            .send(Message::Close(Some(CloseFrame {
                code: GatewayCloseCode::PolicyViolation.into(),
                reason: "IDENTIFY expected".into(),
            })))
            .await
            .ok();
        return Err(());
    };

    let Message::Text(text) = maybe_ident else {
        ws_sink
            .send(Message::Close(Some(CloseFrame {
                code: GatewayCloseCode::InvalidPayload.into(),
                reason: "Invalid IDENTITY payload".into(),
            })))
            .await
            .ok();
        return Err(());
    };

    let Ok(GatewayMessage::Identify(payload)) = serde_json::from_str(&text) else {
        ws_sink
            .send(Message::Close(Some(CloseFrame {
                code: GatewayCloseCode::InvalidPayload.into(),
                reason: "Invalid IDENTITY payload".into(),
            })))
            .await
            .ok();
        return Err(());
    };

    let Ok(token) = Token::validate(app.clone(), payload.token.expose_secret()).await else {
        ws_sink
            .send(Message::Close(Some(CloseFrame {
                code: GatewayCloseCode::PolicyViolation.into(),
                reason: "Invalid token".into(),
            })))
            .await
            .ok();
        return Err(());
    };

    let user_id = token.data().user_id();
    let Some(user) = User::fetch(app, user_id).await else {
        ws_sink
            .send(Message::Close(Some(CloseFrame {
                code: GatewayCloseCode::PolicyViolation.into(),
                reason: "User not found".into(),
            })))
            .await
            .ok();
        return Err(());
    };

    Ok(user)
}

// FIXME
// the heartbeating system holds a reference to the handle
// therefore nobody can get a mutable reference to any handle until the heartbeating system can get cancelled
// which can only get cancelled at an await point
// aka the next heartbeat cycle
// so the connection deadass stalls until the next heartbeat cycle
// at which point the heartbeat loop is finally cancelled by tokio, the lock is dropped, the handle is removed, and your connection is finally dropped

/// Handle the heartbeat mechanism for a given user
///
/// This function will only return if the user failed to send a valid heartbeat within the timeframe
///
/// ## Arguments
///
/// * `app` - The shared application state
/// * `heartbeat_interval` - The interval at which heartbeats should be received from the user
/// * `user_id` - The ID of the user to receive heartbeats from
async fn handle_heartbeating(app: SharedState, user_id: Snowflake, heartbeat_interval: Duration) {
    loop {
        let Some(mut recv) = app.gateway.peers.get(&user_id).map(|h| h.get_receiver()) else {
            return;
        };
        let sleep_task = tokio::spawn(tokio::time::sleep(heartbeat_interval + Duration::from_secs(5)));
        // Wait for a single heartbeat message
        let heartbeat_task = tokio::spawn(async move {
            loop {
                let msg = recv.recv().await.map_err(|_| GatewayCloseCode::InvalidPayload)?;
                if let GatewayMessage::Heartbeat = msg {
                    return Ok(()); // Heartbeat received, we're good
                }
            }
        });

        // Close if either the time runs out or an invalid payload is received
        let should_close: Result<(), GatewayCloseCode> = tokio::select! {
            _ = sleep_task => Err(GatewayCloseCode::PolicyViolation),
            ret = heartbeat_task => ret.unwrap_or(Err(GatewayCloseCode::ServerError)),
        };

        if let Err(close_code) = should_close {
            let reason = match close_code {
                GatewayCloseCode::InvalidPayload => "Invalid payload".into(),
                GatewayCloseCode::PolicyViolation => "No HEARTBEAT received within timeframe".into(),
                _ => "Unknown error".into(),
            };

            app.gateway.drop_session(user_id, close_code, reason);
            break;
        }

        app.gateway.send_to(user_id, GatewayEvent::HeartbeatAck);
    }
}

/// Send the READY event, all GUILD_CREATE events, and dispatch a PRESENCE_UPDATE event for this user
///
/// ## Arguments
///
/// * `app` - The shared application state
/// * `user` - The user to send the READY event to
/// * `ws_sink` - The sink for sending messages to the user
async fn send_ready(app: SharedState, user: User, ws_sink: Arc<Mutex<SplitSink<WebSocket, Message>>>) {
    let guilds = Guild::fetch_all_for_user(app.clone(), user.id())
        .await
        .expect("Failed to fetch guilds during socket connection handling");

    // Send READY
    ws_sink
        .lock()
        .await
        .send(Message::Text(
            serde_json::to_string(&GatewayEvent::Ready(ReadyPayload::new(user.clone(), guilds.clone()))).unwrap(),
        ))
        .await
        .ok();

    // Send GUILD_CREATE events for all guilds the user is in
    for guild in guilds {
        let payload = GuildCreatePayload::from_guild(app.clone(), guild)
            .await
            .expect("Failed to fetch guild payload data");
        ws_sink
            .lock()
            .await
            .send(Message::Text(
                serde_json::to_string(&GatewayEvent::GuildCreate(payload)).unwrap(),
            ))
            .await
            .ok();
    }

    // Send the presence update for the user if they were not invisible when last logging off
    match user.last_presence() {
        Presence::Offline => {}
        _ => {
            app.gateway
                .dispatch(GatewayEvent::PresenceUpdate(PresenceUpdatePayload {
                    user_id: user.id(),
                    presence: *user.last_presence(),
                }));
        }
    }
}

/// Forward events received through the ConnectionHandle receiver to the user
///
/// ## Arguments
///
/// * `user_id` - The ID of the user to send events to
/// * `receiver` - The receiver for incoming gateway responses to send
/// * `ws_sink` - The sink for sending messages to the user
async fn send_events(
    user_id: Snowflake,
    mut receiver: UnboundedReceiverStream<GatewayResponse>,
    ws_sink: Arc<Mutex<SplitSink<WebSocket, Message>>>,
) {
    while let Some(payload) = receiver.next().await {
        match payload {
            GatewayResponse::Close(code, reason) => {
                ws_sink
                    .lock()
                    .await
                    .send(Message::Close(Some(CloseFrame {
                        code: code.into(),
                        reason: reason.into(),
                    })))
                    .await
                    .ok();
                break;
            }
            GatewayResponse::Event(event) => {
                let message = Message::Text(serde_json::to_string(&event).unwrap());
                if let Err(e) = ws_sink.lock().await.send(message).await {
                    tracing::warn!("Error sending event to user {}: {}", user_id, e);
                    break;
                }
            }
        }
    }
}

/// Parse & forward events received through the socket to the ConnectionHandle sender
///
/// ## Arguments
///
/// * `user_id` - The ID of the user to receive events for
/// * `ws_stream` - The stream for receiving messages from the user
/// * `ws_sink` - The sink for sending messages to the user
async fn receive_events(
    user_id: Snowflake,
    mut ws_stream: SplitStream<WebSocket>,
    ws_sink: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    broadcaster: Arc<broadcast::Sender<GatewayMessage>>,
) {
    while let Some(msg) = ws_stream.next().await {
        // Close if the user sends a close frame
        if let Ok(Message::Close(f)) = msg {
            tracing::debug!("Received close frame from user {}: {:?}", user_id, f);
            break;
        }
        // Otherwise attempt to parse the message and send it
        let Ok(Message::Text(text)) = msg else {
            ws_sink
                .lock()
                .await
                .send(Message::Close(Some(CloseFrame {
                    code: GatewayCloseCode::Unsupported.into(),
                    reason: "Unsupported message encoding".into(),
                })))
                .await
                .ok();
            break;
        };

        match serde_json::from_str::<GatewayRequest>(&text) {
            Ok(req) => {
                let GatewayRequest::Message(msg) = req;
                broadcaster.send(msg).ok();
            }
            Err(e) => {
                ws_sink
                    .lock()
                    .await
                    .send(Message::Close(Some(CloseFrame {
                        code: GatewayCloseCode::InvalidPayload.into(),
                        reason: format!("Invalid request payload: {}", e).into(),
                    })))
                    .await
                    .ok();
                break;
            }
        }
    }
}

/// Handle a new websocket connection
///
/// ## Arguments
///
/// * `app` - The shared application state
/// * `socket` - The websocket connection to handle
async fn handle_connection(app: SharedState, socket: WebSocket) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    // Handle handshake and get user
    let Ok(user) = handle_handshake(app.clone(), &mut ws_sink, &mut ws_stream).await else {
        ws_sink.reunite(ws_stream).unwrap().close().await.ok();
        return;
    };

    tracing::debug!("Connected: {} ({})", user.username(), user.id());

    let (sender, receiver) = mpsc::unbounded_channel::<GatewayResponse>();
    let (broadcaster, _) = broadcast::channel::<GatewayMessage>(100);
    let broadcaster = Arc::new(broadcaster);

    // turn receiver into a stream for easier handling
    let receiver = UnboundedReceiverStream::new(receiver);

    let user_id_i64: i64 = user.id().into();

    let guild_ids = sqlx::query!("SELECT guild_id FROM members WHERE user_id = $1", user_id_i64)
        .fetch_all(app.db.pool())
        .await
        .expect("Failed to fetch guilds during socket connection handling")
        .into_iter()
        .map(|row| row.guild_id.into())
        .collect::<HashSet<Snowflake>>();

    // Add user to peermap
    app.gateway.add_handle(
        user.id(),
        ConnectionHandle::new(sender, broadcaster.clone(), guild_ids.clone()),
    );

    let user = user.include_presence(app.clone()).await;
    let user_id = user.id();

    // We want to use the same sink in multiple tasks, so we wrap it in an Arc<Mutex>
    let ws_sink = Arc::new(Mutex::new(ws_sink));

    // Send READY and guild creates to user
    let send_ready = tokio::spawn(send_ready(app.clone(), user.clone(), ws_sink.clone()));

    let send_events = tokio::spawn(send_events(user_id, receiver, ws_sink.clone()));
    let receive_events = tokio::spawn(receive_events(user_id, ws_stream, ws_sink, broadcaster));
    let handle_heartbeat = tokio::spawn(handle_heartbeating(
        app.clone(),
        user_id,
        Duration::from_millis(HEARTBEAT_INTERVAL),
    ));

    tokio::select! {
        _ = send_events => {},
        _ = receive_events => {},
        _ = handle_heartbeat => {},
    }

    send_ready.abort();

    // Disconnection logic
    app.gateway.remove_handle(user.id());
    tracing::debug!("Disconnected: {} ({})", user.username(), user.id());

    // Refetch presence in case it changed
    let presence = User::fetch_presence(app.clone(), &user)
        .await
        .expect("Failed to fetch presence");

    // Send presence update to OFFLINE
    match presence {
        Presence::Offline => {}
        _ => {
            app.gateway
                .dispatch(GatewayEvent::PresenceUpdate(PresenceUpdatePayload {
                    user_id: user.id(),
                    presence: Presence::Offline,
                }));
        }
    }
}
