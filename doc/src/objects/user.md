# User

A user represents a guild-agnostic platform-user. To get a user's guild-specific information, see [Member](member.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `Snowflake` | The user's snowflake ID |
| username | `String` | The user's username, must conform to regex `^([a-zA-Z0-9]\|[a-zA-Z0-9][a-zA-Z0-9]*(?:[._][a-zA-Z0-9]+)*[a-zA-Z0-9])$` |
| display_name | `String?` | The user's display name. If not set, the `username` should be displayed. |
| presence | `String?` | The user's presence, this field is only present in `GUILD_CREATE` and `READY` gateway events. |

### Possible values for presence

- `"ONLINE"`
- `"IDLE"`
- `"BUSY"`
- `"OFFLINE"`

## Example payload

```json
{
    "id": "123456789123456789",
    "username": "among_us",
    "display_name": "Among Us",
    "presence": "ONLINE"
}
```
