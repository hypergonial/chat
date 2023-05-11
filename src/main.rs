use std::{
    collections::HashMap,
    env,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, stream::TryStreamExt, StreamExt};

use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message;

// Mapping of Peer Address: Sender
type PeerMap = Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Payload>>>>;

/// A JSON payload that can be sent over the websocket.
#[derive(Serialize, Deserialize, Debug, Clone)]
enum Payload {
    /// A chat message.
    Message(ChatMessage),
    /// A peer has joined the chat.
    MemberJoin(String),
    /// A peer has left the chat.
    MemberLeave(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ChatMessage {
    author: String,
    content: String,
}

/// Send the given payload to all peers except the one at the given address.
///
/// # Arguments
///
/// * `addr` - The address of the peer that sent the payload. They will not receive this payload.
/// * `peer_map` - The map of peers to send the payload to.
/// * `payload` - The payload to send.
fn send_payload_to_peers(addr: SocketAddr, peer_map: &PeerMap, payload: Payload) {
    let peers = peer_map.lock().unwrap();

    // Send to everyone else except us.
    let broadcast_recipients = peers
        .iter()
        .filter(|(peer_addr, _)| peer_addr != &&addr)
        .map(|(_, sender)| sender);

    for sender in broadcast_recipients {
        sender.unbounded_send(payload.clone()).unwrap_or(());
    }
}

/// Handle and maintain a websocket connection from a peer.
///
/// # Arguments
///
/// * `peers` - The map of peers to send payloads between.
/// * `raw_stream` - The raw TCP stream to the peer.
/// * `addr` - The address of the peer.
async fn handle_connection(peers: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    // Convert the raw TCP stream into a websocket stream.
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("Connected: {}", addr);

    // Create a channel for this peer, and insert the sender into the peermap.
    let (sender, receiver) = unbounded();
    peers.lock().unwrap().insert(addr, sender);

    // Split the websocket stream into a sink and a stream.
    let (outgoing, incoming) = ws_stream.split();

    // Push member join payload
    send_payload_to_peers(addr, &peers, Payload::MemberJoin(addr.to_string()));

    // Forward payloads sent by this peer to other peers
    let broadcast_incoming = incoming.try_for_each(|msg| {
        // Websockets may send an empty payload on closing the connection.
        if msg.len() == 0 {
            return future::ok(());
        }

        let payload = serde_json::from_str::<Payload>(
            msg.to_text().expect("Failed to parse payload into UTF-8."),
        )
        .expect("Failed to serialize into a valid payload type.");

        println!("{:?}", payload);
        send_payload_to_peers(addr, &peers, payload);

        future::ok(())
    });

    // Forward messages received by this peer from others to the output stream
    let receive_from_others = receiver
        .map(|msg| {
            Ok(Message::Text(
                serde_json::to_string(&msg).expect("Failed to serialize message."),
            ))
        })
        .forward(outgoing);

    // When one of the two futures finishes, stop.
    tokio::select! {
        _ = broadcast_incoming => println!("Broadcast incoming ended"),
        _ = receive_from_others => println!("Receive from others ended"),
    }

    // Push member leave payload
    send_payload_to_peers(addr, &peers, Payload::MemberLeave(addr.to_string()));

    println!("Disconnected: {}", &addr);
    // Remove peer from peermap
    peers.lock().unwrap().remove(&addr);
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));

    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address (is the address already in use?)");

    println!("Listening on: {}", addr);

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(peers.clone(), stream, addr));
    }

    Ok(())
}
