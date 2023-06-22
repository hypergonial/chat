use warp::{filters::BoxedFilter, Filter};

use super::common::needs_token;
use crate::models::{
    appstate::APP,
    auth::Token,
    channel::{Channel, ChannelLike},
    gateway_event::GatewayEvent,
    guild::Guild,
    member::Member,
    rejections::{BadRequest, Forbidden, InternalServerError, NotFound},
    rest::{CreateChannel, CreateGuild},
    snowflake::Snowflake,
    user::User,
};
use crate::utils::traits::{OptionExt, ResultExt};
use crate::{
    dispatch,
    models::{channel::TextChannel, gateway_event::GuildCreatePayload},
};

/// Get all routes under `/guilds`
pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    let create_channel = warp::path!("guilds" / Snowflake / "channels")
        .and(warp::post())
        .and(needs_token())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_channel);

    let create_guild = warp::path!("guilds")
        .and(warp::post())
        .and(needs_token())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_guild);

    let fetch_guild = warp::path!("guilds" / Snowflake)
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_guild);

    let fetch_member = warp::path!("guilds" / Snowflake / "members" / Snowflake)
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_member);

    let fetch_member_self = warp::path!("guilds" / Snowflake / "members" / "@self")
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_member_self);

    let add_member = warp::path!("guilds" / Snowflake / "members")
        .and(warp::post())
        .and(needs_token())
        .and_then(create_member);

    let leave_guild = warp::path!("guilds" / Snowflake / "members" / "@self")
        .and(warp::delete())
        .and(needs_token())
        .and_then(leave_guild);

    create_channel
        .or(create_guild)
        .or(fetch_guild)
        .or(fetch_member)
        .or(fetch_member_self)
        .or(add_member)
        .or(leave_guild)
        .boxed()
}

/// Create a new guild and return the guild data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `payload` - The [`CreateGuild`] payload, containing the guild name
///
/// ## Returns
///
/// * [`Guild`] - A JSON response containing the created [`Guild`] object
///
/// ## Dispatches
///
/// * [`GatewayEvent::GuildCreate`] - Dispatched when the guild is created
///
/// ## Endpoint
///
/// POST `/guilds`
async fn create_guild(token: Token, payload: CreateGuild) -> Result<impl warp::Reply, warp::Rejection> {
    let guild = Guild::from_payload(payload, token.data().user_id()).await;

    if let Err(e) = guild.commit().await {
        tracing::error!("Failed to commit guild to database: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }

    if let Err(e) = guild.create_member(token.data().user_id()).await {
        tracing::error!("Failed to add guild owner to guild: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }

    let general = TextChannel::new(Snowflake::gen_new().await, guild.id(), "general".to_string());
    if let Err(e) = general.commit().await {
        tracing::error!("Failed to commit channel to database: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }
    let member = Member::fetch(token.data().user_id(), guild.id())
        .await
        .expect("Member should have been created");

    APP.gateway.write().await.add_member(token.data().user_id(), guild.id());

    dispatch!(GatewayEvent::GuildCreate(GuildCreatePayload::new(
        guild.clone(),
        vec![member.clone()],
        vec![general.clone().into()]
    )));

    Ok(warp::reply::with_status(
        warp::reply::json(&guild),
        warp::http::StatusCode::CREATED,
    ))
}

/// Create a new channel in a guild and return the channel data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `guild_id` - The ID of the guild to create the channel in
/// * `payload` - The [`CreateChannel`] payload, containing the channel name
///
/// ## Returns
///
/// * [`Channel`] - A JSON response containing the created [`Channel`] object
///
/// ## Dispatches
///
/// * [`GatewayEvent::ChannelCreate`] - To all guild members
///
/// ## Endpoint
///
/// POST `/guilds/{guild_id}/channels`
async fn create_channel(
    guild_id: Snowflake,
    token: Token,
    payload: CreateChannel,
) -> Result<impl warp::Reply, warp::Rejection> {
    let user = User::fetch(token.data().user_id())
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to fetch user from database")?;

    let guild = Guild::fetch(guild_id)
        .await
        .or_reject(NotFound::new("The requested guild does not exist."))?;

    if guild.owner_id() != user.id() {
        return Err(warp::reject::custom(Forbidden::new(
            "You are not the owner of this guild.",
        )));
    }

    let channel = Channel::from_payload(payload, guild_id).await;

    channel
        .commit()
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to commit channel to database")?;

    dispatch!(GatewayEvent::ChannelCreate(channel.clone()));

    Ok(warp::reply::with_status(
        warp::reply::json(&channel),
        warp::http::StatusCode::CREATED,
    ))
}

/// Fetch a guild's data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `guild_id` - The ID of the guild to fetch
///
/// ## Returns
///
/// * [`Guild`] - A JSON response containing the fetched [`Guild`] object
///
/// ## Endpoint
///
/// GET `/guilds/{guild_id}`
async fn fetch_guild(guild_id: Snowflake, token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    Member::fetch(token.data().user_id(), guild_id)
        .await
        .or_reject(Forbidden::new("You are not a member of this guild."))?;

    let guild = Guild::fetch(guild_id).await.ok_or_else(|| {
        tracing::error!("Failed to fetch guild from database");
        warp::reject::custom(InternalServerError::db())
    })?;

    Ok(warp::reply::with_status(
        warp::reply::json(&guild),
        warp::http::StatusCode::OK,
    ))
}

/// Fetch a member's data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `guild_id` - The ID of the guild the member is in
///
/// ## Returns
///
/// * [`Member`] - A JSON response containing the fetched [`Member`] object
///
/// ## Endpoint
///
/// GET `/guilds/{guild_id}/members/{member_id}`
async fn fetch_member(
    guild_id: Snowflake,
    member_id: Snowflake,
    token: Token,
) -> Result<impl warp::Reply, warp::Rejection> {
    // Check if the user is in the channel's guild
    Member::fetch(token.data().user_id(), guild_id)
        .await
        .or_reject(Forbidden::new("Not permitted to view resource"))?;

    let member = Member::fetch(member_id, guild_id)
        .await
        .or_reject(NotFound::new("Member does not exist or is not available."))?;

    Ok(warp::reply::with_status(
        warp::reply::json(&member),
        warp::http::StatusCode::OK,
    ))
}

/// Fetch the current user's member data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `guild_id` - The ID of the guild the member is in
///
/// ## Returns
///
/// * [`Member`] - A JSON response containing the fetched [`Member`] object
///
/// ## Endpoint
///
/// GET `/guilds/{guild_id}/members/@self`
async fn fetch_member_self(guild_id: Snowflake, token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let member = Member::fetch(token.data().user_id(), guild_id)
        .await
        .ok_or_else(|| warp::reject::custom(BadRequest::new("Member does not exist or is not available.")))?;

    Ok(warp::reply::with_status(
        warp::reply::json(&member),
        warp::http::StatusCode::OK,
    ))
}

/// Add the token-holder to a guild.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `guild_id` - The ID of the guild to add the user to
///
/// ## Returns
///
/// * [`Member`] - A JSON response containing the created [`Member`] object
///
/// ## Dispatches
///
/// * [`GatewayEvent::GuildCreate`] - For the user who joined the guild
/// * [`GatewayEvent::MemberCreate`] - For all members already in the guild
///
/// ## Endpoint
///
/// POST `/guilds/{guild_id}/members`
async fn create_member(guild_id: Snowflake, token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let guild = Guild::fetch(guild_id).await.ok_or_else(warp::reject::not_found)?;
    if let Err(e) = guild.create_member(token.data().user_id()).await {
        tracing::error!(message = "Failed to add user to guild", user = %token.data().user_id(), guild = %guild_id, error = %e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }

    let member = Member::fetch(token.data().user_id(), guild_id)
        .await
        .expect("A member should have been created");

    // Create payload seperately as it needs read access to gateway
    let gc_payload = GatewayEvent::GuildCreate(
        GuildCreatePayload::from_guild(guild)
            .await
            .or_reject(InternalServerError::db())
            .unwrap(),
    );

    // Send GUILD_CREATE to the user who joined
    APP.gateway.write().await.send_to(member.user().id(), gc_payload);

    // Add the member to the gateway's cache
    APP.gateway.write().await.add_member(member.user().id(), guild_id);

    // Dispatch the member create event to all guild members
    dispatch!(GatewayEvent::MemberCreate(member.clone()));

    Ok(warp::reply::with_status(
        warp::reply::json(&member),
        warp::http::StatusCode::CREATED,
    ))
}

/// Remove the token-holder from a guild.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `guild_id` - The ID of the guild to remove the user from
///
/// ## Returns
///
/// * `()` - An empty response
///
/// ## Dispatches
///
/// * [`GatewayEvent::GuildRemove`] - For the user who left the guild
/// * [`GatewayEvent::MemberRemove`] - For all members still in the guild
///
/// ## Endpoint
///
/// DELETE `/guilds/{guild_id}/members/@self`
async fn leave_guild(guild_id: Snowflake, token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let guild = Guild::fetch(guild_id).await.ok_or_else(warp::reject::not_found)?;
    let member = Member::fetch(token.data().user_id(), guild_id)
        .await
        .or_reject(NotFound::new("The requested member was not found."))?;

    if member.user().id() == guild.owner_id() {
        return Err(warp::reject::custom(Forbidden::new("Cannot leave owned guild.")));
    }

    if let Err(e) = guild.remove_member(token.data().user_id()).await {
        tracing::error!(message = "Failed to remove user from guild", user = %token.data().user_id(), guild = %guild_id, error = %e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }

    // Remove the member from the gateway's cache
    APP.gateway
        .write()
        .await
        .remove_member(token.data().user_id(), guild_id);
    // Dispatch the member remove event
    dispatch!(GatewayEvent::MemberRemove(member.clone()));

    // Send GUILD_REMOVE to the user who left
    APP.gateway
        .write()
        .await
        .send_to(member.user().id(), GatewayEvent::GuildRemove(guild));

    Ok(warp::reply::with_status(
        warp::reply(),
        warp::http::StatusCode::NO_CONTENT,
    ))
}
