FROM rust:latest as builder
# Create a dummy project to cache dependencies
WORKDIR /usr/src
RUN cargo new backend
WORKDIR /usr/src/backend
# Copy the Cargo.toml and build the dependencies
COPY Cargo.toml ./
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ARG BUILD_TYPE=release
RUN if [ "$BUILD_TYPE" = "release" ]; then cargo build --release; else cargo build; fi
# Cleanup
RUN rm src/*.rs
# Copy the source files and finish build
COPY . .
RUN if [ "$BUILD_TYPE" = "release" ]; then cargo install --path .; else cargo install --path . --debug; fi

# Copy the binary into a smaller image
FROM debian:buster-slim
RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/backend /usr/local/bin/backend

CMD ["backend"]
