# User

A user represents a guild-agnostic platform-user. To get a user's guild-specific information, see [Member](member.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `Snowflake` | The user's snowflake ID |
| username | `String` | The user's username, must conform to regex `^([a-zA-Z0-9]\|[a-zA-Z0-9][a-zA-Z0-9]*(?:[._][a-zA-Z0-9]+)*[a-zA-Z0-9])$` |
| display_name | `String?` | The user's display name. If not set, the `username` should be displayed. |
| avatar_hash | `String?` | The user's avatar hash. |
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
    "avatar_hash": "12345678901234567890_png",
    "presence": "ONLINE"
}
```

## Fetching the user's avatar

To fetch the avatar file contents, you must first construct a valid S3 URL. This URL is constructed as follows:

```http
http://<minio_host>:<minio_port>/users/<user_id>/<avatar_hash>.<avatar_ext>
```

Where:

- `<minio_host>` is the host of the MinIO instance, this is `localhost` if you're running the application locally.
- `<minio_port>` is the port of the MinIO instance, this is `9000` if you're running the application locally.
- `<user_id>` is the ID of the user.
- `<avatar_hash>` is avatar hash included with the user object.
- `<avatar_ext>` is the last part of the avatar hash when split on `'_'`

Simply submit a `GET` request to this URL to fetch the file contents. The endpoint is publicly accessible, so no authentication is required.

