FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

# The generated recipe.json should only change if the dependencies changed
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BUILD_TYPE=release
# Build dependencies - this is cached
COPY --from=planner /app/recipe.json recipe.json
RUN <<EOF
  if [ "$BUILD_TYPE" = "release" ]; then
    cargo chef cook --release --recipe-path recipe.json;
  else
    cargo chef cook --recipe-path recipe.json;
  fi
EOF
# Build application
COPY . .
RUN <<EOF
  if [ "$BUILD_TYPE" = "release" ]; then
    cargo build --release;
  else
    cargo build;
  fi
EOF
# Copy it to the same location regardless of build type
RUN <<EOF
  if [ "$BUILD_TYPE" = "release" ]; then
    cp /app/target/release/chat-backend /app/target;
  else
    cp /app/target/debug/chat-backend /app/target;
  fi
EOF

# We do not need the Rust toolchain to run the binary!
FROM debian:bookworm-slim AS runtime
WORKDIR /app
# Required by AWS-SDK which in turn needs rustls to verify the certificates
RUN <<EOF
  apt-get update
  apt-get install -y ca-certificates
  rm -rf /var/lib/apt/lists/*
EOF

COPY --from=builder /app/target/chat-backend /usr/local/bin
ENTRYPOINT ["/usr/local/bin/chat-backend"]
