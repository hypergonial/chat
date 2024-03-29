FROM rust:latest as builder
# Create a dummy project to cache dependencies
WORKDIR /usr/src
RUN cargo new backend
WORKDIR /usr/src/backend
# Copy the Cargo.toml and build the dependencies
COPY Cargo.toml ./
ARG BUILD_TYPE=release
RUN if [ "$BUILD_TYPE" = "release" ]; then cargo build --release; else cargo build; fi
# Cleanup
RUN rm src/*.rs
# Copy the source files and finish build
COPY . .
RUN if [ "$BUILD_TYPE" = "release" ]; then cargo install --path .; else cargo install --path . --debug; fi

# Copy the binary into a smaller image
FROM debian:bookworm-slim
RUN apt-get update
# Required by AWS-SDK which in turn needs rustls to verify the certificates
RUN apt-get install -y ca-certificates
RUN rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/chat-backend /usr/local/bin/chat-backend

CMD ["chat-backend"]
