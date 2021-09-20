# Rust as the base image
FROM rust:1 as build

# Create a new empty shell project
RUN USER=root cargo new --bin mirror-cache
WORKDIR /mirror-cache

# Copy our manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# Build only the dependencies to cache them
RUN cargo build --release
RUN rm src/*.rs

# Copy the source code
COPY ./src ./src

# Build for release.
RUN rm ./target/release/deps/mirror_cache*
RUN cargo build --release

# The final base image, rust image is also base on debian:bullseye
FROM debian:bullseye-slim
RUN apt-get update
RUN apt-get install -y openssl ca-certificates
RUN update-ca-certificates

# Copy from the previous build
COPY --from=build /mirror-cache/target/release/mirror-cache /bin/mirror-cache
WORKDIR /app/
# Run the binary
CMD ["mirror-cache"]
