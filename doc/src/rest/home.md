# REST API

The REST API is the main way of interacting with the Chat API. It can be used to query information about and modify all [objects](../objects/home.md) the client has access to.

## Authentication flow

The REST API uses JWT tokens for authentication. These tokens are obtained by sending a `POST` to `/api/v1/users/auth` with the following JSON body:

```json
{
    "username": "example",
    "password": "*******"
}
```

> To create a user, see [this section](./users.md#/users).

Upon successfully authenticating, the server will respond with a payload like this one:

```json
{
    "user_id": "123456789123456789",
    "token": "*****************************"
}
```

The `token` field is the JWT token that should be used for authentication. It should be sent in the `Authorization` header of all requests to the REST API as a `Bearer` Authorization. In the case the client sent an invalid or expired token, the server will respond with a `401 Unauthorized` status code, and the client is expected to re-authenticate.

## REST API endpoints

All REST API endpoints are currently located under `/api/v1` unless mentioned otherwise. The following endpoints are available:

| Endpoint |
| -------- |
| [/api/v1/users](./users.md) |
| [/api/v1/channels](./channels.md) |
| [/api/v1/guilds](./guilds.md) |

For a detailed description of each endpoint, see the corresponding section.
