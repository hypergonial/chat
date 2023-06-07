# Member

A member represents a guild-specific platform-user. To get a user's guild-agnostic information, see [User](user.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| user | [`User`](user.md) | The member's user data |
| guild_id | `Snowflake` | The member's guild's snowflake ID |
| nickname | `String?` | The member's nickname |
| joined_at | `int` | The member's join timestamp, as a UNIX timestamp. |

## Example payload

```json
{
    "user": {
        "id": "123456789123456789",
        "username": "among_us",
        "display_name": "Among Us",
        "presence": "ONLINE"
    },
    "guild_id": "123456789123456789",
    "nickname": "Among Us",
    "joined_at": 1630000000000
}
```
