use std::{sync::Arc, time::Duration};

use governor::{
    clock::{QuantaClock, QuantaInstant},
    middleware::NoOpMiddleware,
    state::keyed::DashMapStateStore,
    Quota, RateLimiter,
};
use nonzero_ext::nonzero;
use secrecy::ExposeSecret;
use serde_json::json;
use warp::{
    filters::BoxedFilter,
    http::{header, Method},
    Filter,
};

use super::auth::{generate_hash, validate_credentials};
use super::rejections::handle_rejection;
use crate::models::{
    appstate::APP,
    auth::{Credentials, StoredCredentials, Token},
    channel::{Channel, ChannelLike},
    gateway_event::{GatewayEvent, PresenceUpdatePayload},
    guild::Guild,
    member::{Member, UserLike},
    message::Message,
    rejections::{BadRequest, Forbidden, InternalServerError, NotFound, Unauthorized},
    rest::{CreateChannel, CreateGuild, CreateMessage, CreateUser},
    snowflake::Snowflake,
    user::{Presence, User},
};
use crate::utils::traits::{OptionExt, ResultExt};
use crate::{
    dispatch,
    models::{channel::TextChannel, gateway_event::GuildCreatePayload},
};

type SharedIDLimiter = Arc<RateLimiter<u64, DashMapStateStore<u64>, QuantaClock, NoOpMiddleware<QuantaInstant>>>;

/// A filter that checks for and validates a token.
pub fn needs_token() -> impl Filter<Extract = (Token,), Error = warp::Rejection> + Clone {
    warp::header("authorization").and_then(validate_token)
}

/// A filter that checks for and validates a token, and enforces a rate limit.
pub fn needs_limit(id_limiter: SharedIDLimiter) -> impl Filter<Extract = (Token,), Error = warp::Rejection> + Clone {
    needs_token()
        .and(warp::any())
        .map(move |t| (t, id_limiter.clone()))
        .untuple_one()
        .and_then(validate_limit)
}

// Note: Needs to be async for the `and_then` combinator
/// Validate a token and return the parsed token data if successful.
#[inline]
async fn validate_token(token: String) -> Result<Token, warp::Rejection> {
    Token::validate(&token, "among us")
        .await
        .or_reject(Unauthorized::new("Invalid or expired token"))
}

// Check the limiter with the key being the token's user_id
#[inline]
async fn validate_limit(token: Token, limiter: SharedIDLimiter) -> Result<Token, warp::Rejection> {
    let user_id = token.data().user_id();
    limiter.check_key(&user_id.into()).map_err(|e| {
        warp::reject::custom(BadRequest::new(
            format!("Rate limit exceeded, try again at: {:?}", e.earliest_possible()).as_ref(),
        ))
    })?;
    Ok(token)
}

pub fn get_routes() -> BoxedFilter<(impl warp::Reply,)> {
    // https://javascript.info/fetch-crossorigin
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec![
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::OPTIONS,
            Method::PUT,
            Method::PATCH,
        ])
        .allow_headers(vec![
            header::CONTENT_TYPE,
            header::ORIGIN,
            header::AUTHORIZATION,
            header::CACHE_CONTROL,
        ])
        .max_age(Duration::from_secs(3600));

    let message_create_lim: SharedIDLimiter = Arc::new(RateLimiter::keyed(
        Quota::per_second(nonzero!(5u32)).allow_burst(nonzero!(5u32)),
    ));

    let create_user = warp::path!("users")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_user);

    let login = warp::path!("users" / "auth")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(auth_user);

    let query_self = warp::path!("users" / "@self")
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_self);

    let create_msg = warp::path!("channels" / Snowflake / "messages")
        .and(warp::post())
        .and(needs_limit(message_create_lim))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_message);

    let create_channel = warp::path!("guilds" / Snowflake / "channels")
        .and(warp::post())
        .and(needs_token())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(create_channel);

    let fetch_channel = warp::path!("channels" / Snowflake)
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_channel);

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

    let fetch_self_guilds = warp::path!("users" / "@self" / "guilds")
        .and(warp::get())
        .and(needs_token())
        .and_then(fetch_self_guilds);

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

    let update_presence = warp::path!("users" / "@self" / "presence")
        .and(warp::patch())
        .and(needs_token())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(update_presence);

    let query_username = warp::path!("usernames" / String)
        .and(warp::get())
        .and_then(query_username);

    create_msg
        .or(create_user)
        .or(login)
        .or(query_self)
        .or(create_channel)
        .or(fetch_channel)
        .or(create_guild)
        .or(fetch_guild)
        .or(fetch_member)
        .or(fetch_member_self)
        .or(fetch_self_guilds)
        .or(add_member)
        .or(update_presence)
        .or(query_username)
        .or(leave_guild)
        .recover(handle_rejection)
        .with(cors)
        .boxed()
}

/// Add a new ID-based ratelimiter to the filter.
///
/// ## Arguments
///
/// * `limiter` - The ratelimiter to add
pub fn with_id_limiter(
    limiter: SharedIDLimiter,
) -> impl Filter<Extract = (SharedIDLimiter,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || limiter.clone())
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
        .or_reject_and_log(InternalServerError::db(), "Failed to fetch channel from database")?;

    let member = Member::fetch(token.data().user_id(), channel.guild_id())
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to fetch member from database")?;

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

/// Create a new user and return the user data and token.
///
/// ## Arguments
///
/// * `payload` - The CreateUser payload, containing the username and password
///
/// ## Returns
///
/// * [`User`] - A JSON response containing the created [`User`] object
///
/// ## Endpoint
///
/// POST `/users`
async fn create_user(payload: CreateUser) -> Result<impl warp::Reply, warp::Rejection> {
    let password = payload.password.clone();

    let user = match User::from_payload(payload).await {
        Ok(user) => user,
        Err(e) => {
            tracing::debug!("Invalid user payload: {}", e);
            return Err(warp::reject::custom(BadRequest::new(e.to_string().as_ref())));
        }
    };

    if User::fetch_by_username(user.username()).await.is_some() {
        tracing::debug!("User with username {} already exists", user.username());
        return Err(warp::reject::custom(BadRequest::new(
            format!("User with username {} already exists", user.username()).as_ref(),
        )));
    }

    let credentials = StoredCredentials::new(
        user.id(),
        generate_hash(&password).or_reject_and_log(
            InternalServerError::new("Credential generation failed"),
            "Failed to generate password hash",
        )?,
    );

    // User needs to be committed before credentials to avoid foreign key constraint
    if let Err(e) = user.commit().await {
        tracing::error!("Failed to commit user to database: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    } else if let Err(e) = credentials.commit().await {
        tracing::error!("Failed to commit credentials to database: {}", e);
        return Err(warp::reject::custom(InternalServerError::db()));
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&user),
        warp::http::StatusCode::CREATED,
    ))
}

/// Validate a user's credentials and return a token if successful.
///
/// ## Arguments
///
/// * `credentials` - The user's credentials
///
/// ## Returns
///
/// * `{"user_id": user_id, "token": token}` - A JSON response containing the session token and user_id
///
/// ## Endpoint
///
/// POST `/users/auth`
async fn auth_user(credentials: Credentials) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id = validate_credentials(credentials)
        .await
        .or_reject(Unauthorized::new("Invalid credentials"))?;

    let token = Token::new_for(user_id, "among us").or_reject_and_log(
        InternalServerError::new("Failed to generate token"),
        format!("Failed to generate token for user {}", user_id).as_ref(),
    )?;

    Ok(warp::reply::with_status(
        warp::reply::json(&json!({"user_id": user_id, "token": token.expose_secret()})),
        warp::http::StatusCode::OK,
    ))
}

/// Get the current user's data.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
///
/// ## Returns
///
/// * [`User`] - A JSON response containing the user's data
///
/// ## Endpoint
///
/// GET `/users/@self`
async fn fetch_self(token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let user = User::fetch(token.data().user_id())
        .await
        .or_reject_and_log(InternalServerError::db(), "Failed to fetch user from database")?;

    Ok(warp::reply::with_status(
        warp::reply::json(&user),
        warp::http::StatusCode::OK,
    ))
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

    APP.write().await.gateway.add_member(token.data().user_id(), guild.id());

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
    let member = Member::fetch(member_id, guild_id)
        .await
        .or_reject(NotFound::new("Member does not exist or is not available."))?;

    // Check if the user is in the channel's guild
    Member::fetch(token.data().user_id(), guild_id)
        .await
        .or_reject(Forbidden::new("Not permitted to view resource"))?;

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

/// Fetch a user's guilds.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
///
/// ## Returns
///
/// * [`Vec<Guild>`] - A JSON response containing the fetched [`Guild`] objects
///
/// ## Endpoint
///
/// GET `/users/@self/guilds`
async fn fetch_self_guilds(token: Token) -> Result<impl warp::Reply, warp::Rejection> {
    let guilds = Guild::fetch_all_for_user(token.data().user_id()).await.map_err(|e| {
        tracing::error!(message = "Failed to fetch user guilds from database", user = %token.data().user_id(), error = %e);
        warp::reject::custom(InternalServerError::db())
    })?;

    Ok(warp::reply::with_status(
        warp::reply::json(&guilds),
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

    // Send GUILD_CREATE to the user who joined
    APP.read().await.gateway.send_to(
        member.user().id(),
        GatewayEvent::GuildCreate(
            GuildCreatePayload::from_guild(guild)
                .await
                .or_reject(InternalServerError::db())
                .unwrap(),
        ),
    );

    // Add the member to the gateway's cache
    APP.write().await.gateway.add_member(member.user().id(), guild_id);

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
    APP.write()
        .await
        .gateway
        .remove_member(token.data().user_id(), guild_id);
    // Dispatch the member remove event
    dispatch!(GatewayEvent::MemberRemove(member.clone()));

    // Send GUILD_REMOVE to the user who left
    APP.read().await.gateway.send_to(
        member.user().id(),
        GatewayEvent::GuildRemove(
            guild
    ));

    Ok(warp::reply::with_status(
        warp::reply(),
        warp::http::StatusCode::NO_CONTENT,
    ))
}

/// Update the token-holder's presence.
///
/// ## Arguments
///
/// * `token` - The user's session token, already validated
/// * `new_presence` - The new presence to set
///
/// ## Returns
///
/// * [`Presence`] - A JSON response containing the updated [`Presence`] object
/// 
/// ## Dispatches
/// 
/// * [`GatewayEvent::PresenceUpdate`] - For all members in guilds shared with the user
///
/// ## Endpoint
///
/// PATCH `/users/@self/presence`
pub async fn update_presence(token: Token, new_presence: Presence) -> Result<impl warp::Reply, warp::Rejection> {
    let user_id_i64: i64 = token.data().user_id().into();
    let db = &APP.read().await.db;

    sqlx::query!(
        "UPDATE users SET last_presence = $1 WHERE id = $2",
        new_presence as i16,
        user_id_i64
    )
    .execute(db.pool())
    .await
    .map_err(|e| {
        tracing::error!(message = "Failed to update user presence", user = %token.data().user_id(), error = %e);
        warp::reject::custom(InternalServerError::db())
    })?;

    if APP.read().await.gateway.is_connected(token.data().user_id()) {
        dispatch!(GatewayEvent::PresenceUpdate(PresenceUpdatePayload {
            presence: new_presence,
            user_id: token.data().user_id(),
        }));
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&new_presence),
        warp::http::StatusCode::OK,
    ))
}

/// Check for the existence of a user with the given username.
///
/// ## Arguments
///
/// * `username` - The username to check for
///
/// ## Endpoint
///
/// GET `/users/{username}`
pub async fn query_username(username: String) -> Result<impl warp::Reply, warp::Rejection> {
    let db = &APP.read().await.db;

    sqlx::query!("SELECT id FROM users WHERE username = $1", username)
        .fetch_optional(db.pool())
        .await
        .ok()
        .or_reject(NotFound::new("User not found"))?;

    Ok(warp::reply::reply())
}
