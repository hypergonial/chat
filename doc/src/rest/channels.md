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

## POST

### Summary

Sends a message to a channel.

### Payload

```json
{
    "content": "sus",
    "nonce": "this will be echoed back by the gateway!"
}
```

### Response

The created [Message](../objects/message.md) object.

### Errors

| Code | Description |
| ---- | ----------- |
| 403  | The user is not in the guild the channel is located in. |
| 404  | The channel was not found. |
