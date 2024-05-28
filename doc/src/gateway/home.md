# Gateway

The gateway is a websocket connection that allows for real-time communication with the server. It is used mainly to notify connected clients of events that happen on the server, such as messages being sent, channels being created, etc...

## Connection flow

### Handling Heartbeats

After connecting to the gateway (located at `/gateway/v1`), the client will receive a [`HELLO`](./events.md#hello) event as follows:

```json
{
    "event": "HELLO",
    "data": {
        "heartbeat_interval": 45000
    }
}
```

This includes the heartbeat interval the client should use, in milliseconds. Your client **must** send `HEARTBEAT` events *at least* once every interval or it will be disconnected.

Example `HEARTBEAT`:

```json
{
    "event": "HEARTBEAT"
}
```

If successful, the server should immediately return a `HEARTBEAT_ACK` event.
If the server did not acknowledge a heartbeat then the connection should be assumed dead and the client should disconnect. 

### Authentication

The client is then expected to send an `IDENTIFY` payload, the format of which is as follows:

```json
{
    "event": "IDENTIFY",
    "data": {
        "token": "***********************"
    }
}
```

> Please note that you cannot send a `HEARTBEAT` before an `IDENTIFY`. If you do so, your session will be immediately closed.

The socket will then respond with a [`READY`](./events.md#READY) event, which contains the client's user data, as well as the guilds the client is in.

Once `READY` is received, the client will start receiveing [`GUILD_CREATE`](./events.md#GUILD_CREATE) events for all guilds which they are a member of, which contain the guild's data, as well as all the channels and members in it.
