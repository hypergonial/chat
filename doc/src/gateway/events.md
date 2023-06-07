# Events

All events follow the following format:

```json
{
    "event": "EVENT_NAME",
    "data": {
        "field": "value",
        "another_field": "another_value"
    }
}
```

In the following descriptions, when talking about the `data` field, it is implied that the event is wrapped in an object with an `event` field, as shown above.

## MESSAGE_CREATE

### Summary

Sent when a message is sent in a channel that the currently authenticated user is a member of.

### Data

A [Message](../objects/message.md) object.

## MEMBER_CREATE

### Summary

Sent when a member joins a guild that the currently authenticated user is a member of.

### Data

A [Member](../objects/member.md) object.

## MEMBER_REMOVE

### Summary

Sent when a member leaves a guild that the currently authenticated user is a member of.

### Data

A [Member](../objects/member.md) object.

## GUILD_CREATE

### Summary

Sent when a guild is created or on initial connection. The client is expected to cache the guild member & channel data sent in this event, and update it accordingly when receiving associated events.

### Data

| Field | Type | Description |
| --- | --- | --- |
| `guild` | [`Guild`](../objects/guild.md) | The guild's data. |
| `members` | [`Member[]`](../objects/member.md) | The guild's members. |
| `channels` | [`Channel[]`](../objects/channel.md) | The guild's channels. |

## GUILD_REMOVE

### Summary

Sent when a guild is deleted.

### Data

A [Guild](../objects/guild.md) object.

## CHANNEL_CREATE

### Summary

Sent when a channel is created.

### Data

A [Channel](../objects/channel.md) object.

## CHANNEL_REMOVE

### Summary

Sent when a channel is deleted.

### Data

A [Channel](../objects/channel.md) object.

## READY

### Summary

Sent when the client has successfully authenticated and the server is ready to send events.

### Data

| Field | Type | Description |
| --- | --- | --- |
| `user` | [`User`](../objects/user.md) | The client's user data. |
| `guilds` | [`Guild[]`](../objects/guild.md) | The guilds the client is a member of. |

## INVALID_SESSION

### Summary

Sent when the client's session is invalidated. The client is expected to reconnect and send a new `IDENTIFY` payload. The websocket connection is terminated after this event is sent.

### Data

A `String` containing the reason for the session invalidation.
