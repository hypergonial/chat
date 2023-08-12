# Attachment

Represents a file attached to a [Message](./message.md).

## Fields

| Field | Type | Description |
| --- | --- | --- |
| id | `int` | The attachment's ID, this should determine ordering. |
| filename | `String` | The attachment's filename, including the file extension. |
| content_type | `String` | The attachment's [MIME type](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types). |

## Example payload

```json
{
    "id": 0,
    "filename": "among_us.png",
    "content_type": "image/png"
}
```

## Fetching file contents

To fetch the file contents, you must first construct a valid S3 URL. This URL is constructed as follows:

```http
http://<minio_host>:<minio_port>/attachments/<channel_id>/<message_id>/<attachment_id>/<object>
```

Where:

- `<minio_host>` is the host of the MinIO instance, this is `localhost` if you're running the application locally.
- `<minio_port>` is the port of the MinIO instance, this is `9000` if you're running the application locally.
- `<channel_id>` is the channel ID the message was sent in.
- `<message_id>` is the message ID the attachment belongs to.
- `<attachment_id>` is the attachment ID. This is the `id` field in the attachment object.
- `<object>` is the object name, this is the attachment's filename.

Simply submit a `GET` request to this URL to fetch the file contents. The endpoint is publicly accessible, so no authentication is required.
