# Channel

A channel represents a collection of messages.

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `Snowflake` | The channel's snowflake ID |
| name | `String` | The channel's name |
| type | `String` | The channel's type |
| guild_id | `Snowflake` | The channel's guild's snowflake ID. |

### Channel types

- `"GUILD_TEXT"`

## Example payload

```json
{
    "id": "123456789123456789",
    "name": "general",
    "type": "GUILD_TEXT",
    "guild_id": "123456789123456789"
}
```
