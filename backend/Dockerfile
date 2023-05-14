FROM rust:latest as builder
WORKDIR /usr/src/backend
COPY . .
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo fetch
RUN cargo install --path .

# Copy the binary into a smaller image
FROM debian:buster-slim
RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/backend /usr/local/bin/backend

CMD ["backend"]
