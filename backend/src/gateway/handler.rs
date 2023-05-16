use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use secrecy::ExposeSecret;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::filters::BoxedFilter;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::models::appstate::APP;
use crate::models::auth::Token;
use crate::models::gateway_event::{EventLike, GatewayEvent, GatewayMessage};
use crate::models::snowflake::Snowflake;
use crate::models::user::User;

/// Mapping of <user_id, handle>
pub type PeerMap = HashMap<Snowflake, ConnectionHandle>;

/// A struct containing connection details for a user
///
/// ## Fields
///
/// * `sender` - The sender for sending messages to the client
/// * `guild_ids` - The guilds the user is a member of, this is used to filter events
pub struct ConnectionHandle {
    sender: mpsc::UnboundedSender<GatewayEvent>,
    guild_ids: HashSet<Snowflake>,
}

impl ConnectionHandle {
    /// Create a new connection handle with the given sender and guilds
    ///
    /// ## Arguments
    ///
    /// * `sender` - The sender for sending messages to the client
    /// * `guilds` - The guilds the user is a member of
    pub fn new(sender: mpsc::UnboundedSender<GatewayEvent>, guilds: HashSet<Snowflake>) -> Self {
        ConnectionHandle {
            sender,
            guild_ids: guilds,
        }
    }

    /// Send a message to the client
    #[allow(clippy::result_large_err)]
    pub fn send(&self, message: GatewayEvent) -> Result<(), mpsc::error::SendError<GatewayEvent>> {
        self.sender.send(message)
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
pub struct Gateway {
    /// A map of currently connected users
    peers: PeerMap,
}

impl Gateway {
    pub fn new() -> Self {
        Gateway {
            peers: PeerMap::default(),
        }
    }

    /// Dispatch a new event originating from the given user to all other users
    ///
    /// ## Arguments
    ///
    /// * `payload` - The event payload
    pub fn dispatch(&self, event: GatewayEvent) {
        tracing::info!("Dispatching event: {:?}", event);
        for (uid, handle) in self.peers.iter() {
            // If the event is guild-specific, only send it to users that are members of that guild
            if let Some(event_guild) = event.extract_guild_id() {
                if !handle.guild_ids().contains(&event_guild) {
                    continue;
                }
            }
            if let Err(_disconnected) = handle.send(event.clone()) {
                tracing::warn!("Error dispatching event to user: {}", uid);
            }
        }
    }

    /// Registers a new guild member instance to an existing connection
    pub fn add_member(&mut self, user_id: Snowflake, guild_id: Snowflake) {
        if let Some(handle) = self.peers.get_mut(&user_id) {
            handle.guild_ids_mut().insert(guild_id);
        }
    }

    /// Removes a guild member instance from an existing connection
    pub fn remove_member(&mut self, user_id: Snowflake, guild_id: Snowflake) {
        if let Some(handle) = self.peers.get_mut(&user_id) {
            handle.guild_ids_mut().remove(&guild_id);
        }
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}

/// Get routes for handling the gateway
///
/// ## Returns
///
/// A boxed filter that can be used to handle the gateway
pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    let inject_app = warp::any().map(move || &APP);

    let gateway = warp::path("gateway")
        .and(warp::ws()) // <- The `ws()` filter will prepare Websocket handshake...
        .and(inject_app) // <- Use our shared state...
        .map(|ws: warp::ws::Ws, app: &'static _| {
            ws.on_upgrade(move |socket| handle_connection(app, socket))
            // <- call our handler
        });

    gateway.boxed()
}

/// Wait for and validate the IDENTIFY payload
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
    ws_sink: &mut SplitSink<WebSocket, Message>,
    ws_stream: &mut SplitStream<WebSocket>,
) -> Result<User, ()> {
    // IDENTIFY should be the first message sent
    let Some(Ok(maybe_ident)) = ws_stream.next().await else {
        ws_sink.send(Message::close_with(
            1005_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("IDENTIFY expected".into())).unwrap(),
        )).await.ok();
        return Err(());
    };

    if !maybe_ident.is_text() {
        ws_sink
            .send(Message::close_with(
                1003_u16,
                serde_json::to_string(&GatewayEvent::InvalidSession(
                    "Invalid IDENTIFY payload".into(),
                ))
                .unwrap(),
            ))
            .await
            .ok();
        return Err(());
    }

    let Ok(GatewayMessage::Identify(payload)) = serde_json::from_str(maybe_ident.to_str().unwrap()) else {
        ws_sink.send(Message::close_with(
            1003_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("Invalid IDENTIFY payload".into())).unwrap(),
        )).await.ok();
        return Err(());
    };

    let Ok(token) = Token::validate(payload.token.expose_secret(), "among us").await else {
        ws_sink.send(Message::close_with(
            1008_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("Invalid token".into())).unwrap(),
        )).await.ok();
        return Err(());
    };

    let user_id = token.data().user_id();
    let Some(user) = User::fetch(user_id).await else {
        ws_sink.send(Message::close_with(
            1008_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("User not found".into())).unwrap(),
        )).await.ok();
        return Err(());
    };

    ws_sink
        .send(Message::text(
            serde_json::to_string(&GatewayEvent::Ready(user.clone())).unwrap(),
        ))
        .await
        .ok();
    Ok(user)
}

/// Handle a new websocket connection
///
/// ## Arguments
///
/// * `gateway` - The gateway state
/// * `socket` - The websocket connection to handle
async fn handle_connection(app: &'static APP, socket: WebSocket) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    // Handle handshake and get user
    let Ok(user) = handle_handshake(&mut ws_sink, &mut ws_stream).await else {
        ws_sink.reunite(ws_stream).unwrap().close().await.ok();
        return;
    };

    tracing::info!("Connected: {} ({})", user.username(), user.id());

    let (sender, receiver) = mpsc::unbounded_channel::<GatewayEvent>();
    // turn receiver into a stream for easier handling
    let mut receiver = UnboundedReceiverStream::new(receiver);

    let db = &APP.read().await.db;
    let user_id_i64: i64 = user.id().into();

    let guild_ids = sqlx::query!(
        "SELECT guild_id FROM members WHERE user_id = $1",
        user_id_i64
    )
    .fetch_all(db.pool())
    .await
    .expect("Failed to fetch guilds during socket connection handling")
    .into_iter()
    .map(|row| row.guild_id.into())
    .collect::<HashSet<Snowflake>>();

    // Add user to peermap
    app.write()
        .await
        .gateway
        .peers
        .insert(user.id(), ConnectionHandle::new(sender, guild_ids));

    // The sink needs to be shared between two tasks
    let ws_sink: Arc<Mutex<SplitSink<WebSocket, Message>>> = Arc::new(Mutex::new(ws_sink));
    let ws_sink_clone = ws_sink.clone();

    // Same for user_id, so we only want to copy the ID
    let user_id = user.id();

    // Send dispatched events to the user
    let send_events = tokio::spawn(async move {
        while let Some(payload) = receiver.next().await {
            let message = Message::text(serde_json::to_string(&payload).unwrap());
            if let Err(e) = ws_sink.lock().await.send(message).await {
                tracing::warn!("Error sending event to user {}: {}", user_id, e);
                break;
            }
        }
    });

    // Send a ping every 60 seconds to keep the connection alive
    // TODO: Rework to HEARTBEAT system
    let keep_alive = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = ws_sink_clone.lock().await.send(Message::ping(vec![])).await {
                tracing::info!(
                    "Failed to keep alive socket connection to {}: {}",
                    user_id,
                    e
                );
                break;
            }
        }
    });

    // Listen for a close socket event
    let listen_for_close = tokio::spawn(async move {
        while let Some(msg) = ws_stream.next().await {
            if let Ok(msg) = msg {
                if msg.is_close() {
                    break;
                }
            }
        }
    });

    // Wait for any of the tasks to finish
    tokio::select! {
        _ = send_events => {},
        _ = listen_for_close => {},
        _ = keep_alive => {},
    }

    // Disconnection logic
    app.write().await.gateway.peers.remove(&user.id());
    tracing::info!("Disconnected: {} ({})", user.username(), user.id());
}
