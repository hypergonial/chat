use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::filters::BoxedFilter;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::models::gateway_event::GatewayEvent;

/// Mapping of <user_id, sender>
pub type PeerMap = HashMap<usize, mpsc::UnboundedSender<GatewayEvent>>;
pub type SharedGateway = Arc<RwLock<Gateway>>;

// Counter of users
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(0);

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
    pub fn dispatch(&self, user_id: usize, payload: GatewayEvent) {
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
        .and(warp::ws())
        .and(gateway_filter)
        .map(|ws: warp::ws::Ws, gateway: SharedGateway| {
            ws.on_upgrade(move |socket| handle_connection(gateway, socket))
        });

    gateway.boxed()
}

/// Handle a new websocket connection
///
/// # Arguments
///
/// * `gateway` - The gateway state
/// * `socket` - The websocket connection to handle
async fn handle_connection(gateway: SharedGateway, socket: WebSocket) {
    // assign a new user_id to this connection
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::SeqCst);
    println!("Connected: #{user_id}");

    let (mut ws_sink, mut ws_stream) = socket.split();

    let (sender, receiver) = mpsc::unbounded_channel::<GatewayEvent>();
    // turn receiver into a stream for easier handling
    let mut receiver = UnboundedReceiverStream::new(receiver);

    // Add user to peermap
    gateway.write().await.users.insert(user_id, sender);
    gateway
        .read()
        .await
        .dispatch(user_id, GatewayEvent::MemberJoin(user_id.to_string()));

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
            Ok(payload) => gateway.read().await.dispatch(user_id, payload),
            Err(e) => {
                eprintln!("Error parsing payload: {}", e);
            }
        };
    }

    // Disconnection logic
    gateway.write().await.users.remove(&user_id);
    gateway
        .read()
        .await
        .dispatch(user_id, GatewayEvent::MemberLeave(user_id.to_string()));
    println!("Disconnected: #{user_id}");
}
