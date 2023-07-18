use std::sync::Arc;

use governor::{Quota, RateLimiter};
use nonzero_ext::nonzero;
use warp::{filters::BoxedFilter, Filter};

use super::common::SharedIDLimiter;
use super::common::{needs_limit, needs_token};
use crate::dispatch;
use crate::models::{
    appstate::APP,
    auth::Token,
    channel::{Channel, ChannelLike},
    gateway_event::{DeletePayload, GatewayEvent},
    guild::Guild,
    member::{Member, UserLike},
    message::Message,
    rejections::{Forbidden, InternalServerError, NotFound},
    rest::CreateMessage,
    snowflake::Snowflake,
};
use crate::utils::traits::{OptionExt, ResultExt};

/// Get all routes under `/channels
pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    let message_create_lim: SharedIDLimiter = Arc::new(RateLimiter::keyed(
        Quota::per_second(nonzero!(5u32)).allow_burst(nonzero!(5u32)),
    ));

    let fetch_channel = warp::path!("channels" / Snowflake)
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_channel);

    let delete_channel = warp::path!("channels" / Snowflake)
        .and(warp::delete())
        .and(needs_token())
        .and_then(delete_channel);

    let create_msg = warp::path!("channels" / Snowflake / "messages")
        .and(warp::post())
        .and(needs_limit(message_create_lim))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_message);

    fetch_channel.or(create_msg).or(delete_channel).boxed()
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
async fn fetch_channel(channel_id: Snowflake, token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let channel = Channel::fetch(channel_id)
        .await
        .or_reject(NotFound::new("Channel does not exist or is not available."))?;

    // Check if the user is in the channel's guild
    Member::fetch(token.data().user_id(), channel.guild_id())
        .await
        .or_reject(Forbidden::new("Not permitted to view resource."))?;

    Ok(warp::reply::with_status(
        warp::reply::json(&channel),
        warp::http::StatusCode::OK,
    ))
}

async fn delete_channel(channel_id: Snowflake, token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let channel = Channel::fetch(channel_id)
        .await
        .or_reject(NotFound::new("Channel does not exist or is not available."))?;

    // Check guild owner_id
    let guild = Guild::fetch(channel.guild_id())
        .await
        .or_reject(InternalServerError::db())?;

    if guild.owner_id() != token.data().user_id() {
        return Err(Forbidden::new("Not permitted to delete channel.").into());
    }

    channel.delete().await.or_reject(InternalServerError::db())?;

    dispatch!(GatewayEvent::ChannelRemove(DeletePayload::new(
        channel_id,
        Some(guild.id())
    )));

    Ok(warp::reply::with_status(
        warp::reply::reply(),
        warp::http::StatusCode::NO_CONTENT,
    ))
}

/// Send a new message and return the message data.
///
/// ## Arguments
///
/// * `token` - The authorization token
/// * `payload` - The CreateMessage payload
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
    channel_id: Snowflake,
    token: Token,
    payload: CreateMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let channel = Channel::fetch(channel_id)
        .await
        .or_reject(NotFound::new("Channel does not exist or is not available."))?;

    let member = Member::fetch(token.data().user_id(), channel.guild_id())
        .await
        .or_reject(Forbidden::new("Not permitted to access resource."))?;

    let message = Message::from_payload(UserLike::Member(member), channel_id, payload).await;

    message
        .commit()
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to commit message to database")?;

    dispatch!(GatewayEvent::MessageCreate(message.clone()));
    Ok(warp::reply::with_status(
        warp::reply::json(&message),
        warp::http::StatusCode::CREATED,
    ))
}
