# Guild

A guild represents a collection of [members](member.md) and [channels](channel.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `Snowflake` | The guild's snowflake ID |
| name | `String` | The guild's name |
| owner_id | `Snowflake` | The guild's owner's snowflake ID |
| avatar_hash | `String?` | The guild's avatar hash |

## Example payload

```json
{
    "id": "123456789123456789",
    "name": "Among Us",
    "owner_id": "123456789123456789",
    "avatar_hash": "12345678901234567890_png",
}
```

## Fetching the guild's avatar

To fetch the avatar file contents, you must first construct a valid S3 URL. This URL is constructed as follows:

```http
http://<minio_host>:<minio_port>/guilds/<guild_id>/<avatar_hash>.<avatar_ext>
```

Where:

- `<minio_host>` is the host of the MinIO instance, this is `localhost` if you're running the application locally.
- `<minio_port>` is the port of the MinIO instance, this is `9000` if you're running the application locally.
- `<guild_id>` is the ID of the guild.
- `<avatar_hash>` is avatar hash included with the guild object.
- `<avatar_ext>` is the last part of the avatar hash when split on `'_'`

Simply submit a `GET` request to this URL to fetch the file contents. The endpoint is publicly accessible, so no authentication is required.
