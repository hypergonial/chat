# /channels

# /channels/\{channel_id\}

## GET

### Summary

Gets a channel's data.

### Response

A [Channel](../objects/channel.md) object.

### Errors

| Code | Description |
| ---- | ----------- |
| 403  | The user is not in the guild the channel is located in. |
| 404  | The channel was not found. |

## DELETE

### Summary

Deletes a channel.

### Errors

| Code | Description |
| ---- | ----------- |
| 403  | The user has no permission to delete the channel. |
| 404  | The channel was not found. |

# /channels/\{channel_id\}/messages

## GET

### Summary

Fetch a sequence of messages from a channel.

### Query Parameters

| Name | Type | Description |
| ---- | ---- | ----------- |
| before | snowflake? | Get messages before this message ID. |
| after | snowflake? | Get messages after this message ID. |
| limit | integer? | The maximum number of messages to return. Capped at 100, defaults to 50. |

### Response

An array of [Message](../objects/message.md) objects.

## POST

### Summary

Sends a message to a channel.

### Payload

This endpoint expects a `multipart/form-data` payload. The following fields are supported:

| Name | Type | Description |
| ---- | ---- | ----------- |
| json | application/json | Valid json that represents the message's textually representable information |
| attachment-0..9 | application/octet-stream | A file to attach to the message. The `filename` field is mandatory. |

> Note: While both `json` and `attachment` are optional, at least one of them **must** be present.

Example:

```http
POST /channels/123/messages HTTP/1.1
Content-Type: multipart/form-data; boundary=--------------------------1234567890

----------------------------1234567890
Content-Disposition: form-data; name="json"
Content-Type: application/json

{
    "content": "Hello, world!",
    "nonce": "catch me catch me catch me catch.."
}
----------------------------1234567890
Content-Disposition: form-data; name="attachment-0"; filename="cat.png"
Content-Type: image/png

<cat.png bytes>
----------------------------1234567890
Content-Disposition: form-data; name="attachment-1"; filename="dog.gif"
Content-Type: image/gif

<dog.gif bytes>
```

### Response

The created [Message](../objects/message.md) object.

### Errors

| Code | Description |
| ---- | ----------- |
| 403  | The user is not in the guild the channel is located in. |
| 404  | The channel was not found. |
