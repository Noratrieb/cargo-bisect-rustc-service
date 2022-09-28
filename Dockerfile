FROM rust as build

RUN rustup toolchain install nightly
RUN rustup default nightly
RUN rustup target add x86_64-unknown-linux-musl

RUN apt-get update
RUN apt-get install musl-tools -y

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src
RUN echo "fn main() {}" > src/main.rs

RUN cargo build --release -Zsparse-registry

COPY src ./src
COPY index.html index.html

# now rebuild with the proper main
RUN touch src/main.rs
RUN cargo build --release -Zsparse-registry

### RUN
FROM rust

RUN cargo install cargo-bisect-rustc

WORKDIR /app

COPY --from=build /app/target/release/cargo-bisect-rustc-service cargo-bisect-rustc-service

CMD ["/app/cargo-bisect-rustc-service"]
