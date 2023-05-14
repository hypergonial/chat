use std::collections::HashMap;
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

use crate::dispatch;
use crate::models::appstate::APP;
use crate::models::auth::Token;
use crate::models::gateway_event::{GatewayEvent, GatewayMessage};
use crate::models::snowflake::Snowflake;
use crate::models::user::User;

/// Mapping of <user_id, sender>
pub type PeerMap = HashMap<Snowflake, mpsc::UnboundedSender<GatewayEvent>>;

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
    pub fn dispatch(&self, payload: GatewayEvent) {
        tracing::info!("Dispatching event: {:?}", payload);
        for (uid, sender) in self.peers.iter() {
            if let Err(_disconnected) = sender.send(payload.clone()) {
                tracing::warn!("Error dispatching event to user: {}", uid);
            }
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

    // Add user to peermap
    app.write().await.gateway.peers.insert(user.id(), sender);
    dispatch!(GatewayEvent::MemberJoin(user.id().to_string()));

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
    dispatch!(GatewayEvent::MemberLeave(user.id().to_string()));
    tracing::info!("Disconnected: {} ({})", user.username(), user.id());
}
