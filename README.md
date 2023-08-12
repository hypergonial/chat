# Chat

A small prototype repo I made to mess with websockets and Rust. It probably has several security flaws and is highly incomplete.

## Why?

Why not?

## Usage

Firstly, rename `.env.example` and fill it out by providing a valid postgres dsn, MinIO root credentials, and a random string for the session secret.

Then, we need to generate a session token for the admin user in MinIO. To do this, start up the application using `docker compose up` (starting certain components in this state will fail, this is normal) and then
visit `http://localhost:9001` in your webbrowser. Log in using the credentials you provided in the `.env` file, navigate to access keys, and generate a new key. Copy the access key and secret key into the `.env` file.

Then, run `docker compose up` to start the backend, database and MinIO instances.

## Contributing

If you're working with database-related code, set the git hooks directory to `.githooks` using `git config core.hooksPath .githooks`. This ensures that the snapshot for sqlx is up to date.
