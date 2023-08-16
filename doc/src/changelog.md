# Changelog

Only breaking/important changes are listed here. For a full list of changes, see the [commit history](https://github.com/hypergonial/chat/commits/main/).

## 2023.08.16-1

- Added envvar `APP_SECRET` to the `.env` file. This is used to sign & decode the JWTs that are sent to clients. It is recommended to generate a random string and use that as the secret.

## 2023.08.15-1

- Migrated the entire backend from `warp` to `axum`.
- All existing REST routes are now prefixed with `/api/v1/`, so for example, to create a channel, you would send a `POST` request to `/api/v1/guilds/{guild_id}/channels`.
- The gateway endpoint has changed from `/gateway` to `/gateway/v1`
- The formatting of the `Authorization` header has changed to be in line with the http spec. You must preprend the token with `Bearer` before sending it to mark it as a bearer token.

## 2023.08.12-1

- Changed the container names to omit the `chat-` prefix. You may need to update your `.env` file's `POSTGRES_HOST` by changing it to `db`.
- Added attachment support to messages. Attachments are stored in MinIO, an S3-compatible object storage service. The MinIO instances are automatically started when running `docker compose up` and listen on port `:9000`.
- When creating messages, the endpoint `POST /channels/{channel_id}/messages` now expects `multipart/form-data` instead of `application/json`. Details of this change can be found on the route's [documentation page](./rest/channels.md).

## 2023.08.06-1

- Made `User.display_name` nullable. If the user has no display name, clients are expected to use the username instead.
