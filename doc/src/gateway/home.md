# Gateway

The gateway is a websocket connection that allows for real-time communication with the server. It is used mainly to notify connected clients of events that happen on the server, such as messages being sent, channels being created, etc...

## Authentication flow

After connecting to the gateway (located at `/gateway/v1`), the client is expected to send an `IDENTIFY` payload, the format of which is as follows:

```json
{
    "event": "IDENTIFY",
    "data": {
        "token": "***********************"
    }
}
```

The socket will then respond with a [`READY`](./events.md#READY) event, which contains the client's user data, as well as the guilds the client is in.

Once `READY` is received, the client will start receiveing [`GUILD_CREATE`](./events.md#GUILD_CREATE) events for all guilds which they are a member of, which contain the guild's data, as well as all the channels and members in it.
