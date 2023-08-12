# Message

Represents a text message sent to a [channel](channel.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `Snowflake` | The message's snowflake ID |
| channel_id | `Snowflake` | The message's channel's snowflake ID |
| author | [`User`](user.md) or [`Member`](member.md) | The message's author's data, this evaluates to `Member` if in a guild context. |
| content | `String` | The message's content |
| nonce | `String?` | The message's nonce, this may be used by clients to identify their sent messages. It is `null` in all cases except in the `MESSAGE_CREATE` gateway event. |
| attachments | [`Attachment`](attachment.md)[] | The message's attachments. |

## Example payload

```json
{
    "id": "123456789123456789",
    "channel_id": "123456789123456789",
    "author": { // Note that you are not guaranteed to get member objects here.
        "user": {
            "id": "123456789123456789",
            "username": "among_us",
            "display_name": "Among Us",
            "presence": "ONLINE"
        },
        "guild_id": "123456789123456789",
        "nickname": "Among Us",
        "joined_at": 1630000000000
    },
    "content": "sus",
    "nonce": "catch me catch me catch me catch..",
    "attachments": [
        {
            "id": 0,
            "filename": "among_us.png",
            "content_type": "image/png",
        },
        {
            "id": 1,
            "filename": "among_us_2.png",
            "content_type": "image/png",
        }
    ]
}
```
