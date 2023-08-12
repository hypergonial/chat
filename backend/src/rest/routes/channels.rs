use std::sync::Arc;

use governor::{Quota, RateLimiter};
use nonzero_ext::nonzero;
use serde::{Deserialize, Serialize};
use warp::{filters::BoxedFilter, multipart::FormData, Filter};

use super::common::SharedIDLimiter;
use super::common::{needs_limit, needs_token};
use crate::models::{
    appstate::APP,
    auth::Token,
    channel::{Channel, ChannelLike},
    gateway_event::{DeletePayload, GatewayEvent},
    guild::Guild,
    member::{Member, UserLike},
    message::Message,
    rejections::{Forbidden, InternalServerError, NotFound},
    snowflake::Snowflake,
};
use crate::utils::traits::{OptionExt, ResultExt};
use crate::{dispatch, models::rejections::BadRequest};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FetchMessagesQuery {
    limit: Option<u32>,
    before: Option<Snowflake>,
    after: Option<Snowflake>,
}

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
        .and(warp::multipart::form().max_length(10485760))
        .and_then(create_message);

    let fetch_messages = warp::path!("channels" / Snowflake / "messages")
        .and(warp::get())
        .and(needs_token())
        .and(warp::query::<FetchMessagesQuery>())
        .and_then(fetch_messages);

    fetch_channel
        .or(create_msg)
        /* .or(fetch_messages) */
        .or(delete_channel)
        .boxed()
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
    channel_id: Snowflake,
    token: Token,
    payload: FormData,
) -> Result<impl warp::Reply, warp::Rejection> {
    let channel = Channel::fetch(channel_id)
        .await
        .or_reject(NotFound::new("Channel does not exist or is not available."))?;

    let member = Member::fetch(token.data().user_id(), channel.guild_id())
        .await
        .or_reject(Forbidden::new("Not permitted to access resource."))?;

    let message = Message::from_formdata(UserLike::Member(member), channel_id, payload)
        .await
        .or_reject_and_log(
            BadRequest::new("Invalid body. WIP ERROR"),
            "Failed to deserialize message",
        )?;

    message
        .commit()
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to commit message to database")?;

    let message = message.strip_attachment_contents();
    let reply = warp::reply::json(&message);

    dispatch!(GatewayEvent::MessageCreate(message));
    Ok(warp::reply::with_status(reply, warp::http::StatusCode::CREATED))
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
    channel_id: Snowflake,
    token: Token,
    query: FetchMessagesQuery,
) -> Result<impl warp::Reply, warp::Rejection> {
    let channel = Channel::fetch(channel_id)
        .await
        .or_reject(NotFound::new("Channel does not exist or is not available."))?;

    // Check if the user is in the channel's guild
    Member::fetch(token.data().user_id(), channel.guild_id())
        .await
        .or_reject(Forbidden::new("Not permitted to view resource."))?;

    let Channel::GuildText(txtchannel) = channel; /* else {
                                                      return Err(BadRequest::new("Cannot fetch messages from non-textable channel.").into());
                                                  }; */

    let messages = txtchannel
        .fetch_messages(query.limit, query.before, query.after)
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to fetch messages from database")?;

    Ok(warp::reply::with_status(
        warp::reply::json(&messages),
        warp::http::StatusCode::OK,
    ))
}
