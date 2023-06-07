# Guild

A guild represents a collection of [members](member.md) and [channels](channel.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `Snowflake` | The guild's snowflake ID |
| name | `String` | The guild's name |
| owner_id | `Snowflake` | The guild's owner's snowflake ID |

## Example payload

```json
{
    "id": "123456789123456789",
    "name": "Among Us",
    "owner_id": "123456789123456789"
}
```
