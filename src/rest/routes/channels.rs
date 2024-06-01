use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;

use crate::models::{
    auth::Token,
    channel::{Channel, ChannelLike},
    errors::RESTError,
    gateway_event::GatewayEvent,
    member::UserLike,
    message::Message,
    snowflake::Snowflake,
    state::App,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FetchMessagesQuery {
    limit: Option<u32>,
    before: Option<Snowflake<Message>>,
    after: Option<Snowflake<Message>>,
}

/* let message_create_lim: SharedIDLimiter = Arc::new(RateLimiter::keyed(
    Quota::per_second(nonzero!(5u32)).allow_burst(nonzero!(5u32)),
)); */

pub fn get_router() -> Router<App> {
    Router::new()
        .route("/channels/:channel_id", get(fetch_channel))
        .route("/channels/:channel_id", delete(delete_channel))
        .route("/channels/:channel_id/messages", post(create_message))
        .route("/channels/:channel_id/messages", get(fetch_messages))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(8 * 1024 * 1024 /* 8mb */))
}

/// Fetch a channel's data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `channel_id` - The ID of the channel to fetch
///
/// ## Returns
///
/// * [`Channel`] - A JSON response containing the fetched [`Channel`] object
///
/// ## Endpoint
///
/// GET `/channels/{channel_id}`
async fn fetch_channel(
    Path(channel_id): Path<Snowflake<Channel>>,
    State(app): State<App>,
    token: Token,
) -> Result<Json<Channel>, RESTError> {
    let channel = app.ops().fetch_channel(channel_id).await.ok_or(RESTError::NotFound(
        "Channel does not exist or is not available.".to_string(),
    ))?;

    // Check if the user is in the channel's guild
    app.ops()
        .fetch_member(token.data().user_id(), channel.guild_id())
        .await?
        .ok_or(RESTError::Forbidden("Not permitted to view resource.".to_string()))?;

    Ok(Json(channel))
}

/// Delete a channel.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `channel_id` - The ID of the channel to delete
///
/// ## Returns
///
/// * [`StatusCode`] - 204 No Content if successful
///
/// ## Dispatches
///
/// * [`GatewayEvent::ChannelRemove`] - To all members who can view the channel
///
/// ## Endpoint
///
/// DELETE `/channels/{channel_id}`
async fn delete_channel(
    Path(channel_id): Path<Snowflake<Channel>>,
    State(app): State<App>,
    token: Token,
) -> Result<StatusCode, RESTError> {
    let channel = app.ops().fetch_channel(channel_id).await.ok_or(RESTError::NotFound(
        "Channel does not exist or is not available.".into(),
    ))?;

    // Check guild owner_id
    let guild = app
        .ops()
        .fetch_guild(channel.guild_id())
        .await
        .ok_or(RESTError::NotFound("Guild does not exist or is not available.".into()))?;

    if guild.owner_id() != token.data().user_id() {
        return Err(RESTError::NotFound("Not permitted to delete channel.".into()));
    }

    app.ops().delete_channel(&channel).await?;

    app.gateway.dispatch(GatewayEvent::ChannelRemove(channel));

    Ok(StatusCode::NO_CONTENT)
}

/// Send a new message and return the message data.
///
/// ## Arguments
///
/// * `token` - The authorization token
/// * `payload` - The multipart form data
///
/// ## Returns
///
/// * [`Message`] - A JSON response containing a [`Message`] object
///
/// ## Dispatches
///
/// * [`GatewayEvent::MessageCreate`] - To all members who can view the channel
///
/// ## Endpoint
///
/// POST `/channels/{channel_id}/messages`
async fn create_message(
    Path(channel_id): Path<Snowflake<Channel>>,
    State(app): State<App>,
    token: Token,
    payload: Multipart,
) -> Result<(StatusCode, Json<Message>), RESTError> {
    let channel = app.ops().fetch_channel(channel_id).await.ok_or(RESTError::NotFound(
        "Channel does not exist or is not available.".into(),
    ))?;

    let member = app
        .ops()
        .fetch_member(token.data().user_id(), channel.guild_id())
        .await?
        .ok_or(RESTError::Forbidden("Not permitted to access resource.".into()))?;

    let message = Message::from_formdata(&app.config, UserLike::Member(member), channel_id, payload).await?;

    app.ops().update_message(&message).await?;

    let message = message.strip_attachment_contents();
    let reply = Json(message.clone());

    app.gateway.dispatch(GatewayEvent::MessageCreate(message));
    Ok((StatusCode::CREATED, reply))
}

/// Fetch a channel's messages.
///
/// ## Arguments
///
/// * `token` - The authorization token
/// * `channel_id` - The ID of the channel to fetch messages from
/// * `query` - The query parameters
///
/// ## Returns
///
/// * [`Vec<Message>`] - A JSON response containing a list of [`Message`] objects
///
/// ## Endpoint
///
/// GET `/channels/{channel_id}/messages`
async fn fetch_messages(
    Path(channel_id): Path<Snowflake<Channel>>,
    State(app): State<App>,
    token: Token,
    Query(query): Query<FetchMessagesQuery>,
) -> Result<(StatusCode, Json<Vec<Message>>), RESTError> {
    let channel = app.ops().fetch_channel(channel_id).await.ok_or(RESTError::NotFound(
        "Channel does not exist or is not available.".into(),
    ))?;

    // Check if the user is in the channel's guild
    app.ops()
        .fetch_member(token.data().user_id(), channel.guild_id())
        .await?
        .ok_or(RESTError::Forbidden("Not permitted to view resource.".into()))?;

    let messages = app
        .ops()
        .fetch_messages_from(channel_id, query.limit, query.before, query.after)
        .await?;

    Ok((StatusCode::OK, Json(messages)))
}
