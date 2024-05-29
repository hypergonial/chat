FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
ARG BUILD_TYPE=release
RUN if [ "$BUILD_TYPE" = "release" ]; then cargo chef cook --release --recipe-path recipe.json; else cargo chef cook --recipe-path recipe.json; fi
# Build application
COPY . .
RUN if [ "$BUILD_TYPE" = "release" ]; then cargo build --release; else cargo build; fi
RUN if [ "$BUILD_TYPE" = "release" ]; then cp /app/target/release/chat-backend /app/target; else cp /app/target/debug/chat-backend /app/target; fi

# We do not need the Rust toolchain to run the binary!
FROM debian:bookworm-slim AS runtime
WORKDIR /app
# Required by AWS-SDK which in turn needs rustls to verify the certificates
RUN apt-get update
RUN apt-get install -y ca-certificates
RUN rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/chat-backend /usr/local/bin
ENTRYPOINT ["/usr/local/bin/chat-backend"]
