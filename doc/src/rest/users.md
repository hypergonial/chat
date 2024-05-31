# /users

## POST

### Summary

Creates a new user.

### Payload

```json
{
    "username": "example",
    "password": "*******"
}
```

### Response

The created [User](../objects/user.md) object.

### Errors

| Code | Description |
| ---- | ----------- |
| 400  | The username is invalid. |
| 400  | The username is already taken. |

# /users/auth

## POST

### Summary

Authenticates a user, providing an authorization token for use in the REST API and gateway.

### Payload

```json
{
    "username": "example",
    "password": "*******"
}
```

### Response

```json
{
    "user_id": "123456789123456789",
    "token": "*****************************"
}
```

### Errors

| Code | Description |
| ---- | ----------- |
| 401  | The username or password is incorrect. |

# /users/@me

## GET

### Summary

Gets the authenticated user's data.

### Response

A [User](../objects/user.md) object.

## PATCH

### Summary

Edits the authenticated user's data.

### Payload

All fields are optional. All fields specified will be overridden.

```json
{
    "username": "new_username",
    "display_name": "new display name",
    "avatar": "data:image/jpeg;base64,/9j/4AAQSkZJRgABAgAAZABkAAD"
}
```

### Response

The updated [User](../objects/user.md) object.

# /users/@me/guilds

## GET

### Summary

Gets the authenticated user's guilds.

### Response

An array of [Guild](../objects/guild.md) objects.

# /users/@me/presence

## PATCH

### Summary

Updates the authenticated user's presence.

> Note: This endpoint will most likely be removed in favour of updating the user's presence via the gateway.

### Payload

```json
{
    "ONLINE"
}
```

### Response

The updated presence.

```json
{
    "ONLINE"
}
```

# /users/\{username\}

## GET

### Summary

Query the existence of a user with the given username. 
This endpoint is mainly designed for use in registration forms to check if a username is already taken.

### Response

An empty response indicating the user exists.

### Errors

| Code | Description |
| ---- | ----------- |
| 404  | The user with the given username was not found. |
