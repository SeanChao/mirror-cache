FROM rust:latest
WORKDIR /usr/src/myapp
COPY . .
RUN cargo build --release
RUN cargo install --path .
CMD ["/usr/local/cargo/bin/mirror-cache"]
