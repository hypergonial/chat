use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::filters::{BoxedFilter};
use warp::Filter;

use crate::models::socket_event::SocketEvent;

// Counter of users
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(0);

type PeerMap = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<SocketEvent>>>>;

/// Return a warp filter that handles websocket connections, add these routes to enable the gateway
pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)>
{
    let users = PeerMap::default();
    let users = warp::any().map(move || users.clone());

    let gateway = warp::path("gateway")
        .and(warp::ws())
        .and(users)
        .map(|ws: warp::ws::Ws, users| ws.on_upgrade(move |socket| handle_connection(socket, users)));

    gateway.boxed()
}

/// Handle a new websocket connection
/// 
/// # Arguments
/// 
/// * `socket` - The websocket connection
/// * `users` - The peermap of all users
async fn handle_connection(socket: WebSocket, users: PeerMap) {
    // assign a new user_id to this connection
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::SeqCst);
    println!("Connected: #{user_id}");

    let (mut user_sink, mut user_stream) = socket.split();

    let (sender, receiver) = mpsc::unbounded_channel::<SocketEvent>();
    // turn receiver into a stream for easier handling
    let mut receiver_stream = UnboundedReceiverStream::new(receiver);

    // Add user to peermap
    users.write().await.insert(user_id, sender);
    dispatch(user_id, SocketEvent::MemberJoin(user_id.to_string()), &users).await;

    // Messages sent to this user by others should be sent through the socket to them
    tokio::spawn(async move {
        while let Some(payload) = receiver_stream.next().await {
            let message = Message::text(serde_json::to_string(&payload).unwrap());
            if let Err(e) = user_sink.send(message).await {
                eprintln!("Error sending message to user: {}", e);
                break;
            }
        }
    });

    // Messages sent by this user through the socket should be broadcasted to others
    while let Some(raw_json) = user_stream.next().await {
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
        match serde_json::from_str::<SocketEvent>(raw_json) {
            Ok(payload) => dispatch(user_id, payload, &users).await,
            Err(e) => {
                eprintln!("Error parsing payload: {}", e);
            }
        };
    }

    // Disconnection logic
    users.write().await.remove(&user_id);
    dispatch(user_id, SocketEvent::MemberLeave(user_id.to_string()), &users).await;
    println!("Disconnected: #{user_id}");
}

/// Dispatch a new event originating from the given user to all other users
/// 
/// # Arguments
/// 
/// * `user_id` - The id of the user that sent the event
/// * `payload` - The event payload
/// * `users` - The peermap of all users
async fn dispatch(user_id: usize, payload: SocketEvent, users: &PeerMap) {
    println!("Dispatching event: {:?}", payload);
    for (&uid, sender) in users.read().await.iter() {
        if uid != user_id {
            if let Err(_disconnected) = sender.send(payload.clone()) {
                eprintln!("Error sending message to user: {}", uid);
            }
        }
    }
}
