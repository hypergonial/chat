use std::collections::HashMap;
use std::sync::Arc;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::filters::BoxedFilter;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::models::auth::Token;
use crate::models::gateway_event::{GatewayEvent, GatewayMessage};
use crate::models::snowflake::Snowflake;
use crate::models::user::User;

/// Mapping of <user_id, sender>
pub type PeerMap = HashMap<Snowflake, mpsc::UnboundedSender<GatewayEvent>>;
pub type SharedGateway = Arc<RwLock<Gateway>>;

lazy_static! {
    pub static ref GATEWAY: SharedGateway = Arc::new(RwLock::new(Gateway::new()));
}

/// A singleton representing the gateway state
pub struct Gateway {
    /// A map of currently connected users
    users: PeerMap,
}

impl Gateway {
    pub fn new() -> Self {
        Gateway {
            users: PeerMap::default(),
        }
    }

    /// Dispatch a new event originating from the given user to all other users
    ///
    /// # Arguments
    ///
    /// * `user_id` - The id of the user that sent the event
    /// * `payload` - The event payload
    /// * `users` - The peermap of all users
    pub fn dispatch(&self, user_id: Snowflake, payload: GatewayEvent) {
        println!("Dispatching event: {:?}", payload);
        for (&uid, sender) in self.users.iter() {
            if uid != user_id {
                if let Err(_disconnected) = sender.send(payload.clone()) {
                    eprintln!("Error dispatching event to user: {}", uid);
                }
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
/// # Returns
///
/// A boxed filter that can be used to handle the gateway
pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    let gateway_filter = warp::any().map(move || GATEWAY.clone());

    let gateway = warp::path("gateway")
        .and(warp::ws()) // <- The `ws()` filter will prepare Websocket handshake...
        .and(gateway_filter) // <- Use our shared state...
        .map(|ws: warp::ws::Ws, gateway: SharedGateway| {
            ws.on_upgrade(move |socket| handle_connection(gateway, socket))
            // <- call our handler
        });

    gateway.boxed()
}

/// Validate the token header for an incoming websocket connection
async fn handle_handshake(ws_sink: &mut SplitSink<WebSocket, Message>, ws_stream: &mut SplitStream<WebSocket>) -> Result<Token, ()> {
    // IDENTIFY should be the first message sent
    let Some(Ok(maybe_ident)) = ws_stream.next().await else {
        ws_sink.send(Message::close_with(
            1005_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("IDENTIFY expected".into())).unwrap(),
        )).await.ok();
        return Err(());
    };

    if !maybe_ident.is_text() {
        ws_sink.send(Message::close_with(
            1003_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("Invalid IDENTIFY payload".into())).unwrap(),
        )).await.ok();
        return Err(());
    }

    let Ok(GatewayMessage::Identify(token)) = serde_json::from_str(maybe_ident.to_str().unwrap()) else {
        ws_sink.send(Message::close_with(
            1003_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("Invalid IDENTIFY payload".into())).unwrap(),
        )).await.ok();
        return Err(());
    };

    let Ok(token) = Token::decode(&token, "among us") else {
        ws_sink.send(Message::close_with(
            1008_u16,
            serde_json::to_string(&GatewayEvent::InvalidSession("Invalid token".into())).unwrap(),
        )).await.ok();
        return Err(());
    };
    Ok(token)
}

/// Handle a new websocket connection
///
/// # Arguments
///
/// * `gateway` - The gateway state
/// * `socket` - The websocket connection to handle
async fn handle_connection(gateway: SharedGateway, socket: WebSocket) {
    // assign a new user_id to this connection
    let (mut ws_sink, mut ws_stream) = socket.split();
    let Ok(token) = handle_handshake(&mut ws_sink, &mut ws_stream).await else {
        ws_sink.reunite(ws_stream).unwrap().close().await.ok();
        return;
    };
    let user_id = token.data().user_id();
    let user = User::fetch(user_id.into()).await.expect("User not found");

    println!("Connected: {} #({})", user.username, user_id);

    let (sender, receiver) = mpsc::unbounded_channel::<GatewayEvent>();
    // turn receiver into a stream for easier handling
    let mut receiver = UnboundedReceiverStream::new(receiver);

    // Add user to peermap
    gateway.write().await.users.insert(user_id.into(), sender);
    gateway.read().await.dispatch(
        user_id.into(),
        GatewayEvent::MemberJoin(user_id.to_string()),
    );

    // Messages sent to this user by others should be sent through the socket to them
    tokio::spawn(async move {
        while let Some(payload) = receiver.next().await {
            let message = Message::text(serde_json::to_string(&payload).unwrap());
            if let Err(e) = ws_sink.send(message).await {
                eprintln!("Error sending message to user: {}", e);
                break;
            }
        }
    });

    // Messages sent by this user through the socket should be broadcasted to others
    // TODO: Reject payloads once REST api is operational
    while let Some(raw_json) = ws_stream.next().await {
        let raw_json = match raw_json {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Error receiving message from user: {}", e);
                break;
            }
        };
        let Ok(raw_json) = raw_json.to_str() else {
            break;
        };
        match serde_json::from_str::<GatewayEvent>(raw_json) {
            Ok(payload) => gateway.read().await.dispatch(user_id.into(), payload),
            Err(e) => {
                eprintln!("Error parsing payload: {}", e);
            }
        };
    }

    // Disconnection logic
    gateway.write().await.users.remove(&user_id.into());
    gateway.read().await.dispatch(
        user_id.into(),
        GatewayEvent::MemberLeave(user_id.to_string()),
    );
    println!("Disconnected: {} #({})", user.username, user_id);
}
